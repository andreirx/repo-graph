//! METRIC-LANG-COVERAGE-1 — indexed, end-to-end proofs for Rust cyclomatic
//! emission (part B) and the data-driven measurement-coverage caveat (part A).
//!
//! Relocated out of `src/compose.rs` (a >500-line file) per the repo's
//! Structural Guardrail. These exercise only the crate's PUBLIC surface
//! (`index_into_storage` + the public `repo_graph_storage` /
//! `repo_graph_classification` queries), so they belong in the crate's existing
//! `tests/` integration convention (cf. `sb_7b_java_integration.rs`), not inline
//! with the composer.
//!
//! What they add over the unit tests: the rust-extractor unit tests prove
//! metrics are EMITTED at the `ExtractorPort`; the classification/storage tests
//! prove the pure verdict and the SQL over hand-inserted rows. These prove the
//! FULL pipeline — real `.rs`/`.ts` source → `index_into_storage` → persisted
//! `measurements` → `query_measurement_coverage` → `compute_measurement_coverage`
//! — so the (node subtype, file language, measurement key) shapes the coverage
//! query relies on are proven to line up with what the extractors actually emit.

use std::fs;

use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

/// Read one persisted metric value (`{"value":N}`) for the function/method whose
/// `target_stable_key` contains `key_substr`, in `snapshot`.
fn indexed_metric(
    storage: &StorageConnection,
    snapshot: &str,
    kind: &str,
    key_substr: &str,
) -> u64 {
    let rows = storage.query_measurements_by_kind(snapshot, kind).unwrap();
    let row = rows
        .iter()
        .find(|r| r.target_stable_key.contains(key_substr))
        .unwrap_or_else(|| panic!("no {kind} measurement for {key_substr}"));
    let v: serde_json::Value = serde_json::from_str(&row.value_json).unwrap();
    v["value"].as_u64().unwrap()
}

#[test]
fn index_rust_persists_cyclomatic_complexity() {
    // Part B end-to-end: indexing Rust lands cyclomatic_complexity in storage via
    // the shared, language-agnostic `persist_metrics` path, keyed to each
    // function's stable_key, with hand-computed values comparable to C/TS —
    // including a `match` (the dispatch-handler shape this slice targets).
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        r#"pub enum Cmd { A, B, C }

pub fn simple() {}

pub fn branchy(a: bool, b: bool) -> i32 {
    if a && b { 1 } else { 0 }
}

pub fn dispatch(cmd: Cmd) -> u8 {
    match cmd {
        Cmd::A => 1,
        Cmd::B => 2,
        Cmd::C => 3,
        _ => 0,
    }
}
"#,
    )
    .unwrap();

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(root, &mut storage, "r1", &ComposeOptions::default()).unwrap();
    let snap = result.snapshot_uid.clone();

    // Hand-computed cyclomatic values (same decision-point rules as C/TS):
    //   simple:   base                       = 1
    //   branchy:  base + if + &&             = 3   (params a, b       = 2)
    //   dispatch: base + 3 non-wildcard arms = 4   (bare `_` adds 0; param cmd = 1)
    assert_eq!(
        indexed_metric(
            &storage,
            &snap,
            "cyclomatic_complexity",
            "#simple:SYMBOL:FUNCTION"
        ),
        1
    );
    assert_eq!(
        indexed_metric(
            &storage,
            &snap,
            "cyclomatic_complexity",
            "#branchy:SYMBOL:FUNCTION"
        ),
        3
    );
    assert_eq!(
        indexed_metric(
            &storage,
            &snap,
            "cyclomatic_complexity",
            "#dispatch:SYMBOL:FUNCTION"
        ),
        4,
        "match with 3 real arms + bare `_` = base(1) + 3"
    );
    assert_eq!(
        indexed_metric(
            &storage,
            &snap,
            "parameter_count",
            "#dispatch:SYMBOL:FUNCTION"
        ),
        1
    );
    assert_eq!(
        indexed_metric(
            &storage,
            &snap,
            "parameter_count",
            "#branchy:SYMBOL:FUNCTION"
        ),
        2
    );

    // All three kinds land for every measured Rust function (mirrors the TS proof).
    let cc = storage
        .query_measurements_by_kind(&snap, "cyclomatic_complexity")
        .unwrap();
    let pc = storage
        .query_measurements_by_kind(&snap, "parameter_count")
        .unwrap();
    let mnd = storage
        .query_measurements_by_kind(&snap, "max_nesting_depth")
        .unwrap();
    assert_eq!(
        cc.len(),
        3,
        "three Rust functions measured (enum is not a function)"
    );
    assert_eq!(cc.len(), pc.len());
    assert_eq!(cc.len(), mnd.len());
}

#[test]
fn measurement_coverage_caveats_significant_unmeasured_language_end_to_end() {
    // Part A end-to-end CAVEAT proof over INDEXED data — NON-CIRCULAR. Rust is
    // MEASURED here (real fn bodies — the part-B deliverable), and a DIFFERENT
    // language carries the measurement gap, so the caveat can never read as "Rust
    // is unmeasured" (the review-2 objection). The unmeasured vehicle is a
    // TypeScript interface of bare `method_signature`s: ts-extractor emits a
    // METHOD symbol per signature and inserts NO metric (no body to walk —
    // verified in `extract_interface_method`). This is the ONLY real-data shape
    // that leaves a *supported* language unmeasured: every extractor that emits a
    // function BODY also emits its complexity (c/cpp/java/python/rust/ts all do),
    // so a genuinely-unmeasured language is one whose functions are bodyless
    // declarations in this snapshot. In a real repo the MEASURED_SHARE_FLOOR keeps
    // benign stray declarations quiet; here the whole TS surface is bodyless, so
    // the data-driven verdict caveats TypeScript while NOT caveating measured Rust
    // — proving the storage-count ⋈ pure-verdict wiring on real persisted rows.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("package.json"),
        r#"{"name":"m","version":"0.0.0"}"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    // MEASURED: two Rust functions WITH bodies (part-B emission).
    fs::write(
        root.join("src/lib.rs"),
        "pub fn one() -> i32 { 1 }\npub fn two(x: i32) -> i32 { if x > 0 { x } else { 0 } }\n",
    )
    .unwrap();
    // UNMEASURED: a TypeScript interface of three bare method signatures → 3 METHOD
    // nodes, 0 metrics (bodyless — nothing to measure).
    fs::write(
        root.join("src/ports.ts"),
        "export interface Store {\n  save(x: number): void;\n  load(): number;\n  remove(id: number): boolean;\n}\n",
    )
    .unwrap();

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(root, &mut storage, "r1", &ComposeOptions::default()).unwrap();
    let snap = result.snapshot_uid.clone();

    // Real storage counts: Rust 2 funcs / 2 measured; TypeScript 3 methods / 0 measured.
    let counts = storage.query_measurement_coverage(&snap).unwrap();
    let rust = counts
        .iter()
        .find(|c| c.language == "rust")
        .expect("rust coverage row");
    assert_eq!(rust.function_count, 2);
    assert_eq!(
        rust.measured_count, 2,
        "both Rust fn bodies measured (part B): {rust:?}"
    );
    let ts = counts
        .iter()
        .find(|c| c.language == "typescript")
        .expect("typescript coverage row");
    assert_eq!(
        ts.function_count, 3,
        "three interface method signatures = 3 METHOD symbols: {ts:?}"
    );
    assert_eq!(
        ts.measured_count, 0,
        "bare interface signatures carry no complexity measurement: {ts:?}"
    );

    // Data-driven verdict + the serialized `measurement_coverage` block (--json).
    // TS share = 3/5 = 60% (significant + unmeasured → flagged); Rust 2/5 = 40%
    // but fully measured → NOT flagged.
    let cov = repo_graph_classification::measurement_coverage::compute_measurement_coverage(counts);
    let caveat = cov
        .caveat
        .clone()
        .expect("significant unmeasured TypeScript → caveat");
    assert!(caveat.contains("TypeScript (60% of functions)"), "{caveat}");
    assert!(caveat.contains("not yet measured"), "{caveat}");
    assert!(caveat.contains("measured for Rust only"), "{caveat}");
    assert!(caveat.contains("rankings omit it"), "{caveat}");
    assert_eq!(cov.unmeasured, vec!["TypeScript".to_string()]);
    // The non-circular guarantee: the just-measured language is NEVER flagged.
    assert!(
        !cov.unmeasured.contains(&"Rust".to_string()),
        "measured Rust must not be caveated: {:?}",
        cov.unmeasured
    );

    let json = serde_json::to_value(&cov).unwrap();
    assert_eq!(json["kind"], "cyclomatic_complexity");
    assert_eq!(json["unmeasured"], serde_json::json!(["TypeScript"]));
    assert!(json["caveat"].is_string());

    // Surface-shape proof (review-6 item 3): the three complexity surfaces put exactly
    // `MeasurementCoverageBlock::from_result(query).into_json_value()` on the wire. Prove
    // THAT expression — over the same INDEXED data — yields the status-tagged `available`
    // block naming TypeScript, so what metrics/orient/hotspots emit is proven end-to-end,
    // not just the inner verdict. (The `metrics` command is direct-storage, not a daemon
    // method; its rendered stdout is exercised live by the self-dogfood.)
    use repo_graph_classification::measurement_coverage::MeasurementCoverageBlock;
    let block = MeasurementCoverageBlock::from_result(storage.query_measurement_coverage(&snap))
        .into_json_value();
    assert_eq!(block["status"], "available");
    assert_eq!(block["unmeasured"], serde_json::json!(["TypeScript"]));
    assert!(block["caveat"]
        .as_str()
        .unwrap()
        .contains("TypeScript (60% of functions)"));
}

#[test]
fn measurement_coverage_caveat_absent_when_all_indexed_languages_measured() {
    // The "disappears by itself" contract, end-to-end: with Rust now emitting
    // (part B), a Rust+TS snapshot of real bodies has every significant language
    // measured, so the verdict mints NO caveat. NOTE: only the CAVEAT is absent —
    // the `measurement_coverage` block itself is still present (silent, no caveat).
    // Naming this after the caveat (not the block) matches what it asserts: the
    // slice requires the block to remain present-but-quiet on complete coverage.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("package.json"),
        r#"{"name":"m","version":"0.0.0"}"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/util.ts"),
        "export function a(): number { return 1; }\n\
         export function b(x: number): number { if (x > 0) { return x; } return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn one() -> i32 { 1 }\npub fn two(x: i32) -> i32 { if x > 0 { x } else { 0 } }\n",
    )
    .unwrap();

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(root, &mut storage, "r1", &ComposeOptions::default()).unwrap();
    let snap = result.snapshot_uid.clone();

    let counts = storage.query_measurement_coverage(&snap).unwrap();
    let rust = counts
        .iter()
        .find(|c| c.language == "rust")
        .expect("rust coverage row");
    assert_eq!(
        rust.measured_count, rust.function_count,
        "every Rust function measured: {rust:?}"
    );
    assert!(
        rust.function_count >= 2,
        "both Rust functions counted: {rust:?}"
    );

    let cov = repo_graph_classification::measurement_coverage::compute_measurement_coverage(counts);
    assert_eq!(
        cov.caveat, None,
        "no caveat when every language is measured"
    );
    assert!(cov.unmeasured.is_empty());
    let json = serde_json::to_value(&cov).unwrap();
    assert_eq!(json["caveat"], serde_json::Value::Null);
}
