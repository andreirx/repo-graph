//! Deterministic tests for the `metrics` command.
//!
//! Test matrix:
//!   1. Usage error (missing args)
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. Empty results (no measurements of requested kind)
//!   5. --kind filter works correctly
//!   6. --limit caps results
//!   7. --sort value (desc) and --sort target (asc)
//!   8. Malformed value_json is skipped gracefully
//!   9. measurement_coverage block present + `available`, caveat names an
//!      unmeasured language (METRIC-LANG-COVERAGE-1 part A) — proven through the
//!      REAL `rmap metrics` stdout over an indexed mixed-language fixture
//!  10. measurement_coverage block present + `available` but SILENT (no caveat)
//!      when every indexed language is measured ("disappears by itself")

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a temp DB with TS files that produce measurements.
///
/// Layout:
///   src/simple.ts — function simple() {}
///   src/complex.ts — function complex() { if (a) { if (b) { if (c) {} } } }
///
/// Expected measurements:
///   simple: function_length=1, cognitive_complexity=0
///   complex: function_length=1 (single line), cognitive_complexity=6
fn build_metrics_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
    let repo_dir = tempfile::tempdir().unwrap();
    let root = repo_dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
    std::fs::write(root.join("src/simple.ts"), "export function simple() {}\n").unwrap();
    std::fs::write(
        root.join("src/complex.ts"),
        "export function complex() { if (a) { if (b) { if (c) {} } } }\n",
    )
    .unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();
    assert_eq!(result.files_total, 2);

    (repo_dir, db_dir, db_path)
}

/// Build a temp DB mixing MEASURED Rust (real function bodies → part-B cyclomatic
/// emission) with a caller-provided TypeScript source, so the per-language
/// measurement-coverage verdict is exercised through the REAL `rmap metrics`
/// stdout (METRIC-LANG-COVERAGE-1 part A). Uses the same `index_path` file-backed
/// path the `metrics` command reads, so the coverage counts are what the command
/// actually sees.
///
/// The Rust half is fixed: `one()` (cyclomatic 1) + `two()` (base + `if` = 2) —
/// two FUNCTION symbols, both carrying a `cyclomatic_complexity` measurement. The
/// TS half is the variable under test: a bodyless interface leaves TypeScript
/// unmeasured (→ caveat); TS functions with bodies leave it measured (→ silent).
fn build_mixed_lang_db(ts_source: &str) -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
    let repo_dir = tempfile::tempdir().unwrap();
    let root = repo_dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"m","version":"0.0.0"}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn one() -> i32 { 1 }\npub fn two(x: i32) -> i32 { if x > 0 { x } else { 0 } }\n",
    )
    .unwrap();
    std::fs::write(root.join("src/ports.ts"), ts_source).unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();

    (repo_dir, db_dir, db_path)
}

/// A TypeScript interface of three bare method signatures → three METHOD symbols
/// with NO body to walk → TypeScript is left UNMEASURED (share 3/5 = 60% of the
/// snapshot's functions → significant + unmeasured → flagged). This is the only
/// real-data shape that leaves a *supported* language unmeasured, and it is
/// non-circular: the caveat can only name TypeScript, never the measured Rust.
const BODYLESS_TS_INTERFACE: &str =
    "export interface Store {\n  save(x: number): void;\n  load(): number;\n  remove(id: number): boolean;\n}\n";

/// Two TypeScript functions WITH bodies → ts-extractor emits `cyclomatic_complexity`
/// for each, so TypeScript is MEASURED and no language is flagged.
const MEASURED_TS_FUNCTIONS: &str = "export function a(): number { return 1; }\n\
     export function b(x: number): number { if (x > 0) { return x; } return 0; }\n";

// -- 1. Usage error ---------------------------------------------------

#[test]
fn metrics_usage_error() {
    let output = Command::new(binary_path())
        .args(["metrics"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Missing DB ----------------------------------------------------

#[test]
fn metrics_missing_db() {
    let output = Command::new(binary_path())
        .args(["metrics", "/nonexistent.db", "r1"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 3. Repo not found ------------------------------------------------

#[test]
fn metrics_repo_not_found() {
    let (_repo_dir, _db_dir, db_path) = build_metrics_db();

    let output = Command::new(binary_path())
        .args(["metrics", db_path.to_str().unwrap(), "nonexistent-repo"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no snapshot"), "stderr: {}", stderr);
}

// -- 4. Empty results -------------------------------------------------

#[test]
fn metrics_empty_for_unknown_kind() {
    let (_repo_dir, _db_dir, db_path) = build_metrics_db();

    let output = Command::new(binary_path())
        .args([
            "metrics",
            db_path.to_str().unwrap(),
            "r1",
            "--kind",
            "nonexistent_kind",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["count"], 0);
    assert!(result["results"].as_array().unwrap().is_empty());
}

// -- 5. --kind filter -------------------------------------------------

#[test]
fn metrics_kind_filter() {
    let (_repo_dir, _db_dir, db_path) = build_metrics_db();

    let output = Command::new(binary_path())
        .args([
            "metrics",
            db_path.to_str().unwrap(),
            "r1",
            "--kind",
            "cognitive_complexity",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let results = result["results"].as_array().unwrap();
    assert!(
        !results.is_empty(),
        "should have cognitive_complexity measurements"
    );

    // All results should be cognitive_complexity
    for r in results {
        assert_eq!(r["kind"], "cognitive_complexity");
    }
}

// -- 6. --limit caps results ------------------------------------------

#[test]
fn metrics_limit() {
    let (_repo_dir, _db_dir, db_path) = build_metrics_db();

    let output = Command::new(binary_path())
        .args(["metrics", db_path.to_str().unwrap(), "r1", "--limit", "1"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["count"], 1);
    assert_eq!(result["results"].as_array().unwrap().len(), 1);
}

// -- 7. --sort value vs target ----------------------------------------

#[test]
fn metrics_sort_by_value_desc() {
    let (_repo_dir, _db_dir, db_path) = build_metrics_db();

    let output = Command::new(binary_path())
        .args([
            "metrics",
            db_path.to_str().unwrap(),
            "r1",
            "--kind",
            "cognitive_complexity",
            "--sort",
            "value",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let results = result["results"].as_array().unwrap();
    assert!(results.len() >= 2, "should have at least 2 results");

    // complex() should be first (higher complexity)
    let first_key = results[0]["target_stable_key"].as_str().unwrap();
    assert!(
        first_key.contains("complex"),
        "first result should be complex function, got: {}",
        first_key
    );
}

#[test]
fn metrics_sort_by_target_asc() {
    let (_repo_dir, _db_dir, db_path) = build_metrics_db();

    let output = Command::new(binary_path())
        .args([
            "metrics",
            db_path.to_str().unwrap(),
            "r1",
            "--kind",
            "cognitive_complexity",
            "--sort",
            "target",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let results = result["results"].as_array().unwrap();
    let keys: Vec<&str> = results
        .iter()
        .map(|r| r["target_stable_key"].as_str().unwrap())
        .collect();

    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "results should be sorted by target ascending");
}

// -- 8. QueryResult envelope contract ---------------------------------

#[test]
fn metrics_envelope_contract() {
    let (_repo_dir, _db_dir, db_path) = build_metrics_db();

    let output = Command::new(binary_path())
        .args(["metrics", db_path.to_str().unwrap(), "r1"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Verify TS-compatible QueryResult envelope fields.
    assert_eq!(result["command"], "metrics");
    assert!(result["repo"].is_string(), "repo field must be present");
    assert!(
        result["snapshot"].is_string(),
        "snapshot field must be present"
    );
    assert!(
        result["snapshot_scope"] == "full" || result["snapshot_scope"] == "incremental",
        "snapshot_scope must be full or incremental"
    );
    assert!(
        result["basis_commit"].is_null() || result["basis_commit"].is_string(),
        "basis_commit must be string or null"
    );
    assert!(result["stale"].is_boolean(), "stale field must be boolean");
    assert!(result["count"].is_number(), "count field must be number");
    assert!(result["results"].is_array(), "results field must be array");

    // Verify row shape
    let results = result["results"].as_array().unwrap();
    if !results.is_empty() {
        let row = &results[0];
        assert!(row["target_stable_key"].is_string());
        assert!(row["kind"].is_string());
        assert!(row["value"].is_number());
        assert!(row["source"].is_string());
    }
}

// -- 9. measurement_coverage: caveat fires for an unmeasured language --

#[test]
fn metrics_surface_carries_available_coverage_caveat_for_unmeasured_language() {
    // review-7 item 1: prove the ACTUAL `rmap metrics` stdout carries the
    // always-present, data-driven `measurement_coverage` block naming the
    // unmeasured language — end-to-end from an indexed mixed-language fixture,
    // not only the storage/classification units or the orient/hotspots handlers
    // (metrics is a direct-storage command, so its own stdout needed a proof).
    let (_repo_dir, _db_dir, db_path) = build_mixed_lang_db(BODYLESS_TS_INTERFACE);

    let output = Command::new(binary_path())
        .args([
            "metrics",
            db_path.to_str().unwrap(),
            "r1",
            "--kind",
            "cyclomatic_complexity",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // (1) The slice's headline defect, at the metrics surface: Rust now RANKS.
    // Only the (measured) Rust functions carry cyclomatic_complexity here — the
    // bodyless TS methods do not — so the ranking is exactly the two Rust fns,
    // with their hand-computed values reaching the command output.
    let rows = result["results"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "both measured Rust fns rank: {rows:?}");
    for r in rows {
        assert_eq!(r["kind"], "cyclomatic_complexity");
    }
    let mut values: Vec<i64> = rows.iter().map(|r| r["value"].as_i64().unwrap()).collect();
    values.sort_unstable();
    assert_eq!(
        values,
        vec![1, 2],
        "Rust cyclomatic values (one=1, two=1+if=2) reach the surface: {rows:?}"
    );

    // (2) review-7 item 1: the always-present coverage block, `available`, naming
    // the unmeasured language data-drivenly (share computed from the snapshot).
    let cov = &result["measurement_coverage"];
    assert_eq!(
        cov["status"], "available",
        "metrics must carry the coverage block: {result}"
    );
    assert_eq!(
        cov["unmeasured"],
        serde_json::json!(["TypeScript"]),
        "TypeScript (bodyless interface) is the unmeasured vehicle: {cov}"
    );
    let caveat = cov["caveat"].as_str().expect("caveat present");
    assert!(
        caveat.contains("TypeScript (60% of functions)"),
        "caveat names the unmeasured language + share: {caveat}"
    );
    assert!(caveat.contains("not yet measured"), "{caveat}");
    assert!(caveat.contains("rankings omit it"), "{caveat}");
    // Non-circular: the just-measured Rust is NEVER listed as unmeasured. The
    // caveated form is always "<Lang> (NN% of functions)"; the measured lead is
    // "measured for Rust only" (no open paren), so `Rust (` must be absent.
    assert!(
        !caveat.contains("Rust ("),
        "measured Rust must not be caveated as unmeasured: {caveat}"
    );
}

// -- 10. measurement_coverage: present + silent when all measured -----

#[test]
fn metrics_surface_coverage_available_and_silent_when_all_measured() {
    // The "disappears by itself" contract at the command surface: with Rust
    // emitting (part B) and the TS functions carrying bodies, every indexed
    // language is measured, so the block is still PRESENT + `available` but mints
    // NO caveat (the block is only ever absent because a surface has no complexity
    // content — never because coverage happens to be complete).
    let (_repo_dir, _db_dir, db_path) = build_mixed_lang_db(MEASURED_TS_FUNCTIONS);

    let output = Command::new(binary_path())
        .args([
            "metrics",
            db_path.to_str().unwrap(),
            "r1",
            "--kind",
            "cyclomatic_complexity",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Both languages measured → both rank (2 Rust + 2 TS cyclomatic rows).
    assert_eq!(
        result["results"].as_array().unwrap().len(),
        4,
        "all four measured functions rank: {result}"
    );

    let cov = &result["measurement_coverage"];
    assert_eq!(
        cov["status"], "available",
        "block present even when coverage is complete: {result}"
    );
    assert_eq!(
        cov["caveat"],
        serde_json::Value::Null,
        "no caveat when every language is measured: {cov}"
    );
    assert!(
        cov["unmeasured"].as_array().unwrap().is_empty(),
        "nothing flagged: {cov}"
    );
}
