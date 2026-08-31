//! FIND-RANK-1 (§2.1, review-0) — the RANK-WINDOW regression seam, driven through the
//! REAL `ServiceDispatcher::dispatch` `find` surface. This proves the ONE guarantee the
//! prior build (build-1) did NOT: that the DEFAULT (non-`--full`) rendering's visible
//! symbol cap is the GLOBAL top-N under the ratified comparator — not merely the top-N
//! of a lexically-early SQL fetch window.
//!
//! The blocking defect (review-0): the symbol read used to fetch a bounded 200-row
//! window ordered by `(is_test, name ASC)`, THEN re-rank in Rust. Rank precedence weights
//! KIND above name, so 200+ lexically-early non-test lesser-kind matches (`VARIABLE`s)
//! could fill the window and EXCLUDE a prominent production `FUNCTION` whose name sorts
//! late — the comparator never saw it, and it was invisible. `--exact` (which fetches the
//! whole set) hid the bug; DEFAULT mode exposed it. The fix fetches the COMPLETE matching
//! set in every mode and ranks it, so the visible cap is globally correct.
//!
//! This fixture seeds EXACTLY that adversarial shape — 210 non-test `VARIABLE`s whose
//! names (`a_rankwidget_NNN`) sort lexically BEFORE a single prominent production
//! `FUNCTION` (`zRankWidget`) — and asserts, through DEFAULT-mode `find` (no `--exact`),
//! that the prominent symbol leads the visible hits. Under the pre-fix 200-row window the
//! function was row 211 by name and never entered the window, so this test FAILS on the
//! old code and PASSES on the fix (the review-0 must-fail regression obligation).

mod seed_harness;
use seed_harness::*;

use repo_graph_seed::SeedCorpusRead;
use repo_graph_storage::StorageConnection;
use serde_json::json;

/// Number of lexically-early non-test lesser-kind decoys. Must exceed the pre-fix
/// `LIKE_FETCH_WINDOW` (200) so the window is fully consumed by decoys before the
/// prominent symbol is reached in `name ASC` order — the exact exclusion scenario.
const DECOY_VARIABLES: usize = 210;

/// DEFAULT-mode `find` (no `--exact`): a prominent production `FUNCTION` whose name sorts
/// AFTER 210 non-test `VARIABLE` matches must still lead the visible symbol hits. Proves
/// the visible cap is the GLOBAL top-N (kind-weighted), not the top-N of a lexical fetch
/// window (review-0 blocking defect; the must-fail-on-old-code regression).
#[test]
fn default_find_shows_prominent_symbol_behind_200_plus_lexically_early_variables() {
    let _env = SeedEnv::with_endpoint("http://127.0.0.1:9/v1/embeddings"); // model down: facts still answer
    let (d, _root) = isolated_quiet();
    let repo = make_repo(); // helper.ts is a non-test (production) tracked file
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, repo_uid) = coords(&idx);
    let snapshot_uid = idx["snapshot_uid"]
        .as_str()
        .expect("index returns snapshot_uid")
        .to_string();

    // Anchor every seeded node to helper.ts's real file_uid — a NON-TEST production file
    // (`files.is_test = 0`). So the ONLY ranking discriminator among the seeded matches is
    // KIND (§2.1b), which is exactly the axis the window bug got wrong.
    let corpus = StorageConnection::open(&db_path).unwrap();
    let entries = corpus.seed_corpus(&repo_uid).unwrap();
    let helper_uid = entries
        .iter()
        .find(|e| e.path.ends_with("helper.ts"))
        .expect("helper.ts in corpus")
        .file_uid
        .clone();
    drop(corpus);

    // Raw-insert the adversarial node set (same committed-write seam the other fact-class
    // seam tests use: WAL + busy_timeout let the write commit while the daemon holds its
    // cached idle connection; the daemon reads it on the next query). FK enforcement OFF so
    // the fixture need not fabricate parents the read never joins; product code is
    // unaffected (the daemon reads through its own FK-checked connection). All 210 decoys
    // are non-test `VARIABLE`s named `a_rankwidget_NNN` (sort first by `name ASC`); the one
    // prominent match is a non-test `FUNCTION` named `zRankWidget` (sorts last by name).
    let raw = rusqlite::Connection::open(&db_path).expect("raw open for rank-window seed");
    raw.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    raw.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let mut values = String::new();
    for i in 0..DECOY_VARIABLES {
        if !values.is_empty() {
            values.push(',');
        }
        // Zero-padded so lexical `name ASC` order is unambiguous and all sort before 'z'.
        values.push_str(&format!(
            "('nd-var-{i:04}','{snapshot_uid}','{repo_uid}','k_var_{i:04}','SYMBOL','VARIABLE','a_rankwidget_{i:04}','{helper_uid}')"
        ));
    }
    // The single prominent production symbol — a FUNCTION whose name sorts AFTER every decoy.
    values.push_str(&format!(
        ",('nd-fn-prom','{snapshot_uid}','{repo_uid}','k_fn_prom','SYMBOL','FUNCTION','zRankWidget','{helper_uid}')"
    ));
    raw.execute_batch(&format!(
        "INSERT INTO nodes \
           (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, file_uid) \
         VALUES {values};"
    ))
    .expect("seed the rank-window adversarial node set");
    drop(raw); // release before the daemon reads

    // DEFAULT mode (NO `--exact`): this is the mode the 200-row window governed. `find`
    // runs the facts tier synchronously even with the endpoint down.
    let resp = dispatch_ok(
        &d,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "rankwidget" }),
    );

    let facts = resp["facts"].as_array().expect("facts array present");
    let symbol = facts
        .iter()
        .find(|g| g["fact_class"] == "symbol")
        .expect("symbol group present");
    let hits = symbol["hits"].as_array().expect("symbol hits array");

    // The visible cap is the default per-class display budget (8) — a small window over a
    // 211-match set, so this is a genuine "is the winner in the VISIBLE slots" assertion.
    assert!(
        hits.len() <= 8,
        "default rendering caps the symbol class at 8 visible hits, got {}: {symbol}",
        hits.len()
    );

    // The prominent production FUNCTION leads the visible hits (kind weight ranks it above
    // every VARIABLE). Under the pre-fix 200-row window it was excluded from the fetch
    // entirely and would be ABSENT here — this is the regression that must fail on old code.
    let displays: Vec<&str> = hits
        .iter()
        .map(|h| h["display"].as_str().unwrap_or(""))
        .collect();
    assert!(
        displays.contains(&"zRankWidget"),
        "the prominent production FUNCTION is in the VISIBLE cap, not crowded out by 210 \
         lexically-early non-test VARIABLEs: {displays:?}"
    );
    assert_eq!(
        displays.first(),
        Some(&"zRankWidget"),
        "prominent (kind-weighted) symbol leads the visible hits: {displays:?}"
    );
}
