//! TRUNCATION-AUDIT-1 — pipeline-level proof that the meaningful pre-truncation ordering and
//! the `--full` (Budget::Full) escape hatch behave end-to-end through the real `run_explain`
//! use case (the IMPL-2 pattern, exercised via the in-memory fake).
//!
//! The per-comparator determinism + load-bearing properties are unit-tested in
//! `src/ordering.rs`; these tests prove (a) the pipeline actually APPLIES the sort BEFORE the
//! budget cut, (b) the cut is load-bearing (the surviving top-N is the ranked set, not the
//! storage-order prefix), (c) the order is input-order-independent through the pipeline, and
//! (d) `--full` uncaps the list (count == total, nothing truncated, the capped tier is a
//! prefix of the full output).
//!
//! EXPLAIN_IMPORTS (file focus) is the vehicle: it is trivial to seed with > the budget cap,
//! and its order key (`target_file` ASC) makes "ranked vs raw" easy to assert. The same sort
//! seam is shared by every explain item list, so this also exercises the wiring.

mod common;

use common::{FakeAgentStorage, TEST_NOW};
use repo_graph_agent::{run_explain, AgentImportEntry, AgentPathResolution, Budget};
use serde_json::Value;

const FILE: &str = "src/big.ts";
const TOTAL: usize = 60; // > the Large item cap (50), so the cut bites at every non-full tier.

/// Seed a file-focus explain whose file imports `TOTAL` distinct target files, supplied to
/// storage in the given order. `order` lets a test feed forward / reversed / shuffled inputs
/// to prove the ranked output is order-independent.
fn seed_file_with_imports(order: &[usize]) -> FakeAgentStorage {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    fake.path_resolutions.insert(
        ("snap1".into(), FILE.into()),
        AgentPathResolution {
            has_exact_file: true,
            file_stable_key: Some("r1:src/big.ts:FILE".into()),
            has_content_under_prefix: false,
            module_stable_key: None,
        },
    );
    let imports: Vec<AgentImportEntry> = order
        .iter()
        .map(|i| AgentImportEntry {
            // Zero-padded so lexicographic order == numeric order.
            target_file: format!("src/dep{i:02}.ts"),
        })
        .collect();
    fake.file_imports
        .insert(("snap1".into(), FILE.into()), imports);
    fake
}

/// Extract the EXPLAIN_IMPORTS section from a serialized explain result:
/// `(count, items_truncated, target_files_in_emitted_order)`.
fn imports_section(result: &repo_graph_agent::OrientResult) -> (u64, Option<bool>, Vec<String>) {
    let json = serde_json::to_value(result).expect("serialize OrientResult");
    let signal = json["signals"]
        .as_array()
        .expect("signals array")
        .iter()
        .find(|s| s["code"] == Value::String("EXPLAIN_IMPORTS".into()))
        .expect("EXPLAIN_IMPORTS signal present");
    let ev = &signal["evidence"];
    let count = ev["count"].as_u64().expect("count");
    let truncated = ev.get("items_truncated").and_then(|v| v.as_bool());
    let targets = ev["items"]
        .as_array()
        .expect("items array")
        .iter()
        .map(|it| it["target_file"].as_str().expect("target_file").to_string())
        .collect();
    (count, truncated, targets)
}

fn dep(i: usize) -> String {
    format!("src/dep{i:02}.ts")
}

/// The cut is LOAD-BEARING: storage supplies imports in reverse order, so the raw top-50
/// (dep59..dep10) differs from the ranked top-50 (dep00..dep49). The pipeline must emit the
/// RANKED prefix — proving the sort is applied BEFORE truncation, not after (or never).
#[test]
fn explain_imports_truncates_on_ranked_order_not_storage_order() {
    let reverse: Vec<usize> = (0..TOTAL).rev().collect();
    let fake = seed_file_with_imports(&reverse);

    let result = run_explain(&fake, "r1", FILE, Budget::Large, TEST_NOW).unwrap();
    let (count, truncated, targets) = imports_section(&result);

    assert_eq!(
        count, TOTAL as u64,
        "count reflects the FULL set, not the cut"
    );
    assert_eq!(truncated, Some(true), "Large cap (50) < 60 ⇒ truncated");
    assert_eq!(targets.len(), 50, "Large cap keeps 50 items");

    // Ranked top-50 = dep00..dep49 (target_file ASC).
    let expected_ranked: Vec<String> = (0..50).map(dep).collect();
    assert_eq!(
        targets, expected_ranked,
        "surviving items are the RANKED prefix"
    );

    // The raw (storage-order) top-50 would have been dep59..dep10 — prove they differ, so the
    // ordering genuinely changes WHICH items survive (the sanctioned behaviour change).
    let raw_top: Vec<String> = reverse.iter().take(50).map(|i| dep(*i)).collect();
    assert_ne!(targets, raw_top, "ranking must change the surviving subset");
}

/// Input-order independence THROUGH the pipeline: forward, reversed, and shuffled storage
/// orders all yield the identical truncated output (the DR-EXPLAIN-CALLER-ORDER guarantee, so
/// SQLite vs LiveGraph row order cannot change the truncated view).
#[test]
fn explain_imports_order_is_input_independent() {
    let forward: Vec<usize> = (0..TOTAL).collect();
    let reversed: Vec<usize> = (0..TOTAL).rev().collect();
    // A fixed permutation (no RNG): swap pairs around the middle.
    let mut shuffled: Vec<usize> = (0..TOTAL).collect();
    shuffled.swap(0, 59);
    shuffled.swap(7, 33);
    shuffled.swap(12, 48);

    let run = |order: &[usize]| {
        let fake = seed_file_with_imports(order);
        let result = run_explain(&fake, "r1", FILE, Budget::Large, TEST_NOW).unwrap();
        imports_section(&result).2
    };

    let f = run(&forward);
    assert_eq!(
        f,
        run(&reversed),
        "reversed storage order ranks identically"
    );
    assert_eq!(
        f,
        run(&shuffled),
        "shuffled storage order ranks identically"
    );
}

/// `--full` (Budget::Full) uncaps the list: count == total, NOTHING truncated, and the
/// default (capped) tier is a strict prefix of the full output — proving `--full` emits the
/// SAME items, just un-truncated.
#[test]
fn explain_full_emits_all_items_untruncated() {
    let reverse: Vec<usize> = (0..TOTAL).rev().collect();
    let fake = seed_file_with_imports(&reverse);

    let full = run_explain(&fake, "r1", FILE, Budget::Full, TEST_NOW).unwrap();
    let (full_count, full_truncated, full_targets) = imports_section(&full);

    assert_eq!(full_count, TOTAL as u64);
    assert_eq!(
        full_targets.len(),
        TOTAL,
        "--full emits EVERY item (count == total)"
    );
    assert_eq!(
        full_truncated, None,
        "--full sets no truncation flag (nothing was cut)"
    );
    // Full output is the complete ranked list dep00..dep59.
    let expected_full: Vec<String> = (0..TOTAL).map(dep).collect();
    assert_eq!(full_targets, expected_full);

    // The default Large tier is a strict prefix of the full output (same items, truncated).
    let large = run_explain(&fake, "r1", FILE, Budget::Large, TEST_NOW).unwrap();
    let (_, large_truncated, large_targets) = imports_section(&large);
    assert_eq!(large_truncated, Some(true));
    assert_eq!(
        large_targets,
        full_targets[..large_targets.len()],
        "the capped tier is a prefix of --full output (same order, fewer rows)"
    );
}
