//! Unit tests for `boundaries_summary_read` (split from the parent for the 500-line
//! guardrail; wired via `#[path]` per the repo `lib_tests.rs` convention).

use super::*;

fn base_summary() -> serde_json::Value {
    // FRAKTAG-shaped base: 52 boundary-http consumers, 0 providers.
    serde_json::json!({
        "totalSurfaces": 52,
        "totalChannels": 0,
        "byChannelKind": [{"channelKind": "http", "count": 52}],
        "byBoundaryScope": [{"boundaryScope": "unknown", "count": 52}],
        "byDirection": [{"direction": "consumer", "count": 52}],
        "byProtocolFamily": [{"protocolFamily": "http", "count": 52}],
        "byBasis": [{"basis": "api_call", "count": 52}],
        "filesWithBoundaries": ["web/a.ts"]
    })
}

#[test]
fn adjust_scalar_clamps_and_adds() {
    let mut m = serde_json::Map::new();
    m.insert("totalSurfaces".to_string(), serde_json::json!(52));
    adjust_scalar(&mut m, "totalSurfaces", 41); // 52 - 52(bh) + 93(union) = 93
    assert_eq!(m["totalSurfaces"], serde_json::json!(93));
}

#[test]
fn adjust_bucket_replaces_direction_split() {
    // The FRAKTAG contradiction: base says 52 consumer / 0 provider; the
    // reconciled split must be 47 provider / 46 consumer.
    let serde_json::Value::Object(mut map) = base_summary() else {
        unreachable!()
    };
    let mut dir: BTreeMap<String, i64> = BTreeMap::new();
    dir.insert("consumer".to_string(), -52 + 46);
    dir.insert("provider".to_string(), 47);
    adjust_bucket(&mut map, "byDirection", "direction", &dir);
    let arr = map["byDirection"].as_array().unwrap();
    let get = |k: &str| {
        arr.iter()
            .find(|e| e["direction"] == serde_json::json!(k))
            .map(|e| e["count"].as_i64().unwrap())
            .unwrap_or(0)
    };
    assert_eq!(get("provider"), 47);
    assert_eq!(get("consumer"), 46);
}

#[test]
fn empty_deltas_leave_arrays_untouched() {
    // No HTTP in either family → every delta is zero → the summary is
    // byte-identical (the leveldb byte-parity guarantee, in miniature).
    let serde_json::Value::Object(mut map) = base_summary() else {
        unreachable!()
    };
    let snapshot = serde_json::Value::Object(map.clone());
    adjust_bucket(&mut map, "byDirection", "direction", &BTreeMap::new());
    adjust_scalar(&mut map, "totalSurfaces", 0);
    assert_eq!(serde_json::Value::Object(map), snapshot);
}

#[test]
fn composition_acc_files_are_conservative() {
    // A file appears in the test-only file list ONLY if EVERY reconciled row on it was
    // test-only. A mixed file (one production row) stays out — never hide a real file.
    let mut acc = CompositionAcc {
        total_surfaces: 1, // ensure test_only_json is what finish() would emit
        ..CompositionAcc::default()
    };
    acc.note_file("tests/only.rs", true);
    acc.note_file("tests/only.rs", true); // still wholly test-only
    acc.note_file("src/mixed.rs", true); // a test-only row …
    acc.note_file("src/mixed.rs", false); // … and a production row ⇒ keep in headline
    acc.note_file("src/prod.rs", false);
    let json = acc.test_only_json();
    let files: Vec<&str> = json["filesWithBoundaries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        files,
        vec!["tests/only.rs"],
        "only the wholly-test-only file"
    );
}

#[test]
fn unknown_rows_disclose_but_never_demote() {
    // review-2 #1 + binding direction rule: UNKNOWN-composition rows are counted into the
    // `unknown` disclosure (with distinct reasons) — NEVER into the test-only sub-summary,
    // and an unknown row on a file keeps that file OUT of the test-only file list (not hidden).
    let mut acc = CompositionAcc::default();
    acc.note_unknown("no stored is_test fact for vendor/a.ts".to_string());
    acc.note_unknown("no stored is_test fact for vendor/a.ts".to_string()); // deduped
    acc.note_unknown("no stored is_test fact for vendor/b.ts".to_string());
    // A file carrying an unknown row must not demote: note it as non-test-only.
    acc.note_file("vendor/a.ts", false);
    let partition = acc.finish();
    // No test-only surface ⇒ no test_only key (payload stays byte-identical for that half).
    assert!(
        partition.test_only.is_none(),
        "no positive test-only evidence"
    );
    let unknown = partition.unknown.expect("unknown disclosure present");
    assert_eq!(
        unknown["surfaces"],
        serde_json::json!(3),
        "counts every unknown row"
    );
    let reasons: Vec<&str> = unknown["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    // Distinct + sorted (BTreeSet): two reasons, a.ts before b.ts.
    assert_eq!(
        reasons,
        vec![
            "no stored is_test fact for vendor/a.ts",
            "no stored is_test fact for vendor/b.ts"
        ]
    );
}

#[test]
fn no_test_composition_signal_emits_neither_key() {
    // A repo with only production rows (positive evidence, no unknowns) emits NEITHER
    // additive key — the pre-slice byte-identical payload.
    let mut acc = CompositionAcc::default();
    acc.note_file("src/prod.rs", false);
    let partition = acc.finish();
    assert!(partition.test_only.is_none());
    assert!(partition.unknown.is_none());
}

#[test]
fn bucket_array_emits_labeled_positive_counts() {
    let mut m: BTreeMap<String, i64> = BTreeMap::new();
    m.insert("http".to_string(), 5);
    m.insert("socket".to_string(), 0); // never emitted (only-increment invariant guard)
    let arr = bucket_array(&m, "channelKind");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["channelKind"], serde_json::json!("http"));
    assert_eq!(arr[0]["count"], serde_json::json!(5));
}

#[test]
fn bucket_drops_zeroed_entries() {
    let serde_json::Value::Object(mut map) = base_summary() else {
        unreachable!()
    };
    // Remove all 52 api_call, add 93 unknown → api_call gone, unknown present.
    let mut basis: BTreeMap<String, i64> = BTreeMap::new();
    basis.insert("api_call".to_string(), -52);
    basis.insert("unknown".to_string(), 93);
    adjust_bucket(&mut map, "byBasis", "basis", &basis);
    let arr = map["byBasis"].as_array().unwrap();
    assert!(arr
        .iter()
        .all(|e| e["basis"] != serde_json::json!("api_call")));
    assert_eq!(
        arr.iter()
            .find(|e| e["basis"] == serde_json::json!("unknown"))
            .unwrap()["count"],
        serde_json::json!(93)
    );
}
