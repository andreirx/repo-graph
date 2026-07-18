//! RECON-M-R2 gate tests (recon-design-1 §6.1 M-R2 gate column, driven THROUGH SERVING):
//! union ⊇ P verbatim; the ROW/COUNT invariant (`count == rows.len()` across every fixture
//! class); DIVERGENT-CAPTURE (the §4.2 capture redefinition — the M-R1 twin's opposite);
//! EPOCH-MOVED (transient 1: pipeline bytes at the pinned snapshot, NO witness fields, the named
//! movement reason); CAPTURE-FAILED (transient 2: build error retained, pipeline serve);
//! DELTA-PAIR served rows (P=2/S=1 `mixed` + exact occurrences, never `both`; P=1/S=2 one `both`
//! P row + one S-minted `semantic` row, closure + row multiset 1:1); STALE-serving (W-ONE:
//! pipeline bytes, no union fields); collision-withheld pairs NEVER serve; PER-SYMBOL
//! unanswerability inside W-BOTH (§3.6, iterations 2–3 — BOTH unanswerable classes: a
//! Fresh/eligible `Partial` projection AND an `Unavailable` anchor whose FILE is in an eligible
//! partition each SERVE the union with nonzero `unmeasured`, no false row witness, counts
//! summing to `rows.len()`); the ADDED pipeline-only fixture through serving (boundary +
//! uncorroborated shapes, amodx-informed); the R-0/R-1 shapes (LG-less and uncovered-FILE
//! answers byte-equal to today's path); the W-B epoch PIN re-check (EPOCH-MOVED is the pin
//! test; eviction/rebuild is unchanged M-R1 surface).

use serde_json::Value;

use crate::callgraph_cert::test_fixture::{self, Fixture};
use crate::callgraph_cert::{callgraph_cert_eligibility, callgraph_union_eligibility};
use crate::livegraph_feed::{callers_engine_response, Engine, RequestEpoch};

use super::{callees_union_response, callers_union_response, union_serving_enabled};

// ── Helpers ──────────────────────────────────────────────────────────────────────────────────

/// A `RequestEpoch` with an explicit fingerprint (the dispatch arms' capture, made test-drivable).
fn epoch_with(f: &Fixture, fingerprint: Option<String>) -> RequestEpoch {
    let storage = f.state.storage().expect("open storage");
    let snapshot =
        repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, test_fixture::REPO)
            .expect("snapshot query")
            .expect("ready snapshot");
    RequestEpoch {
        snapshot,
        fingerprint,
    }
}

/// The flag-ON capture (ledger-validity-gated) + epoch, exactly as `handle_callers` builds it.
fn captured_epoch(f: &Fixture) -> RequestEpoch {
    let fp = callgraph_union_eligibility(&f.state, &f.snapshot_uid);
    epoch_with(f, fp)
}

fn resolve(f: &Fixture, symbol: &str) -> repo_graph_storage::queries::ResolvedSymbol {
    f.state
        .storage()
        .expect("open storage")
        .resolve_symbol(&f.snapshot_uid, symbol)
        .expect("resolve symbol")
}

fn union_callers(f: &Fixture, epoch: &RequestEpoch, symbol: &str) -> Value {
    let target = resolve(f, symbol);
    let storage = f.state.storage().expect("open storage");
    callers_union_response(&f.state, epoch, &target, || {
        storage.find_direct_callers(&f.snapshot_uid, &target.stable_key, &["CALLS"])
    })
    .expect("callers response")
}

fn union_callees(f: &Fixture, epoch: &RequestEpoch, symbol: &str) -> Value {
    let target = resolve(f, symbol);
    let storage = f.state.storage().expect("open storage");
    callees_union_response(&f.state, epoch, &target, || {
        storage.find_direct_callees(&f.snapshot_uid, &target.stable_key, &["CALLS"])
    })
    .expect("callees response")
}

/// The flag-OFF `Auto` response (today's exact path) for byte-parity comparisons.
fn flag_off_callers(f: &Fixture, symbol: &str) -> Value {
    let fp = callgraph_cert_eligibility(&f.state, &f.snapshot_uid);
    let epoch = epoch_with(f, fp);
    let target = resolve(f, symbol);
    let storage = f.state.storage().expect("open storage");
    callers_engine_response(
        Engine::Auto,
        &f.state,
        &epoch,
        &target,
        || storage.find_direct_callers(&f.snapshot_uid, &target.stable_key, &["CALLS"]),
        symbol,
        "",
    )
    .expect("callers response")
}

fn rows(v: &Value, field: &str) -> Vec<Value> {
    v[field].as_array().expect("rows array").clone()
}

/// The ROW/COUNT invariant (`count == rows.len()`) — asserted on EVERY served answer in this
/// module (the M-R2 gate's cross-fixture invariant test).
fn assert_row_count_invariant(v: &Value, field: &str) {
    assert_eq!(
        v["count"].as_u64().expect("count") as usize,
        rows(v, field).len(),
        "count == rows.len() (the preserved §5.2 boundary contract)"
    );
}

/// Strip the M-R2 additive row fields, for verbatim-P comparisons.
fn strip_witness(mut row: Value) -> Value {
    if let Some(obj) = row.as_object_mut() {
        obj.remove("witness");
        obj.remove("occurrences");
    }
    row
}

fn witness_counts(v: &Value) -> (u64, u64, u64, u64) {
    let w = &v["witness_counts"];
    (
        w["both"].as_u64().expect("both"),
        w["semantic_only"].as_u64().expect("semantic_only"),
        w["syntactic_only"].as_u64().expect("syntactic_only"),
        w["unmeasured"].as_u64().expect("unmeasured"),
    )
}

// ── The flag ─────────────────────────────────────────────────────────────────────────────────

#[test]
fn union_flag_is_exactly_one() {
    // The ratified env-mutating-test convention (diff.rs / platform-paths): only this test
    // touches this var; save/restore the original value. Edition 2021 → set_var/remove_var safe.
    let original = std::env::var_os(super::UNION_SERVING_ENV);
    std::env::remove_var(super::UNION_SERVING_ENV);
    assert!(!union_serving_enabled(), "unset = OFF (the default)");
    std::env::set_var(super::UNION_SERVING_ENV, "0");
    assert!(!union_serving_enabled(), "\"0\" = OFF");
    std::env::set_var(super::UNION_SERVING_ENV, "true");
    assert!(
        !union_serving_enabled(),
        "exactly \"1\" is ON (recorded contract)"
    );
    std::env::set_var(super::UNION_SERVING_ENV, "1");
    assert!(union_serving_enabled());
    match original {
        Some(v) => std::env::set_var(super::UNION_SERVING_ENV, v),
        None => std::env::remove_var(super::UNION_SERVING_ENV),
    }
}

// ── DIVERGENT-CAPTURE (the §4.2 redefinition's named test; the M-R1 twin's opposite) ─────────

#[test]
fn divergent_fixture_captures_a_fingerprint_and_serves_union_in_w_both() {
    // drop_calls: SQLite has NO CALLS row, S holds one strict-Calls edge -> divergent -> RED.
    // Flag-OFF capture (GREEN-gated): None — the M-R1 twin, still true. Flag-ON capture
    // (ledger-validity-gated, verdict-independent): Some — and the union SERVES the
    // `semantic`/`new_pair` instance as an S-minted row.
    let f = test_fixture::build_fixture(true);
    assert_eq!(
        callgraph_cert_eligibility(&f.state, &f.snapshot_uid),
        None,
        "GREEN-gated capture still refuses a divergent graph (flag-off path unchanged)"
    );
    let fp = callgraph_union_eligibility(&f.state, &f.snapshot_uid);
    assert!(
        fp.is_some(),
        "ledger-validity-gated capture is verdict-independent (§4.2)"
    );
    // A successful capture supersedes any retained build failure.
    assert!(f.state.witness_ledger_build_failure.read().is_none());

    let epoch = epoch_with(&f, fp);
    let v = union_callers(&f, &epoch, "calleeFn");
    assert_row_count_invariant(&v, "callers");
    assert_eq!(v["backend_used"], "union");
    assert_eq!(v["fallback_reason"], Value::Null);
    let r = rows(&v, "callers");
    assert_eq!(r.len(), 1, "P holds 0 rows; the S-only instance mints 1");
    // The S-minted row: §5.2 shape — enriched name/file, NULL-not-zero locations (§3.7-4
    // retired on this path), CALLS kind (honest under the kind-partitioned projection).
    assert_eq!(r[0]["witness"], "semantic");
    assert_eq!(r[0]["stable_key"], test_fixture::caller_key());
    assert_eq!(r[0]["name"], "callerFn");
    assert_eq!(r[0]["file"], "src/a.ts");
    assert_eq!(r[0]["line"], Value::Null, "unknown is null, never 0");
    assert_eq!(r[0]["column"], Value::Null);
    assert_eq!(r[0]["edge_type"], "CALLS");
    assert_eq!(witness_counts(&v), (0, 1, 0, 0));
}

// ── union ⊇ P verbatim (named test) + DELTA-PAIR served rows ─────────────────────────────────

#[test]
fn union_contains_every_p_row_verbatim_and_in_order() {
    let f = test_fixture::build_multiplicity_fixture(2, 1);
    let epoch = captured_epoch(&f);
    assert!(
        epoch.fingerprint.is_some(),
        "divergent graph still captures"
    );

    let target = resolve(&f, "calleeFn");
    let storage = f.state.storage().expect("open storage");
    let p_rows = storage
        .find_direct_callers(&f.snapshot_uid, &target.stable_key, &["CALLS"])
        .expect("sqlite rows");
    let p_json: Vec<Value> = p_rows
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();

    let v = union_callers(&f, &epoch, "calleeFn");
    assert_row_count_invariant(&v, "callers");
    let served: Vec<Value> = rows(&v, "callers")
        .into_iter()
        .filter(|r| r["witness"] != "semantic") // S-minted rows are the only non-P rows
        .map(strip_witness)
        .collect();
    assert_eq!(
        served, p_json,
        "P rows serve VERBATIM (all fields, original order) — union ⊇ P (guard 1)"
    );
}

#[test]
fn delta_pair_p2_s1_serves_mixed_rows_with_exact_occurrences_never_both() {
    let f = test_fixture::build_multiplicity_fixture(2, 1);
    let epoch = captured_epoch(&f);
    let v = union_callers(&f, &epoch, "calleeFn");
    assert_row_count_invariant(&v, "callers");
    let r = rows(&v, "callers");
    assert_eq!(r.len(), 2, "MAX(2,1) = 2 — the served MAX rule");
    for row in &r {
        assert_eq!(
            row["witness"], "mixed",
            "a P-excess delta pair's rows are `mixed`, NEVER `both` (R-RAT-5)"
        );
        assert_eq!(row["occurrences"]["confirmed"], 1);
        assert_eq!(row["occurrences"]["total"], 2);
    }
    // Instance counts (1:1 with the row multiset): 1 both + 1 syntactic/multiplicity.
    assert_eq!(witness_counts(&v), (1, 0, 1, 0));
}

#[test]
fn delta_pair_p1_s2_serves_one_both_row_plus_one_minted_semantic_row() {
    let f = test_fixture::build_multiplicity_fixture(1, 2);
    let epoch = captured_epoch(&f);
    let v = union_callers(&f, &epoch, "calleeFn");
    assert_row_count_invariant(&v, "callers");
    let r = rows(&v, "callers");
    assert_eq!(r.len(), 2, "count 2: MAX(1,2) — closure + row multiset 1:1");
    assert_eq!(
        r[0]["witness"], "both",
        "every P occurrence is corroborated (min = p) — `both` stands"
    );
    assert_eq!(
        r[1]["witness"], "semantic",
        "the S-excess instance MINTS a `semantic` row (iteration 6 — count == rows preserved)"
    );
    assert_eq!(r[1]["line"], Value::Null);
    assert_eq!(witness_counts(&v), (1, 1, 0, 0));
    // Both directions: the callees projection of the same pair serves symmetrically.
    let v2 = union_callees(&f, &epoch, "callerFn");
    assert_row_count_invariant(&v2, "callees");
    let r2 = rows(&v2, "callees");
    assert_eq!(r2.len(), 2);
    assert_eq!(r2[0]["witness"], "both");
    assert_eq!(r2[1]["witness"], "semantic");
    assert_eq!(r2[1]["stable_key"], test_fixture::callee_key());
}

// ── EPOCH-MOVED (transient 1) + STALE-serving (W-ONE) ────────────────────────────────────────

#[test]
fn epoch_moved_between_capture_and_read_serves_pipeline_at_pinned_snapshot() {
    let f = test_fixture::build_fixture(false);
    let epoch = captured_epoch(&f);
    assert!(epoch.fingerprint.is_some());
    // The fingerprint MOVES after capture (any witness movement — here: mark_stale flips the
    // freshness bit, which is IN the fingerprint).
    f.state
        .livegraph
        .write()
        .as_mut()
        .expect("livegraph resident")
        .mark_stale("p");
    let v = union_callers(&f, &epoch, "calleeFn");
    assert_row_count_invariant(&v, "callers");
    assert_eq!(
        v["backend_used"], "sqlite",
        "pipeline at the pinned snapshot"
    );
    assert_eq!(
        v["fallback_reason"], "LiveGraphEpochMoved",
        "the NAMED movement reason (§4.2 transient 1) — no longer folded into Unavailable"
    );
    assert!(
        v.get("witness_counts").is_none(),
        "NO witness fields on a failed-soft answer"
    );
    for row in rows(&v, "callers") {
        assert!(row.get("witness").is_none());
    }
    // Rows are the pipeline bytes at the pinned snapshot.
    let storage = f.state.storage().expect("open storage");
    let target = resolve(&f, "calleeFn");
    let p_json: Vec<Value> = storage
        .find_direct_callers(&f.snapshot_uid, &target.stable_key, &["CALLS"])
        .expect("sqlite rows")
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    assert_eq!(rows(&v, "callers"), p_json);
}

#[test]
fn stale_partition_serves_pipeline_bytes_with_no_union_fields_w_one() {
    // Stale BEFORE capture: the partition leaves W-BOTH eligibility (W-ONE `stale`); the ledger
    // at the stale fingerprint holds no classification rows for it; serving is today's exact
    // stale fallback (pipeline bytes + `LiveGraphStale`).
    let f = test_fixture::build_fixture(false);
    f.state
        .livegraph
        .write()
        .as_mut()
        .expect("livegraph resident")
        .mark_stale("p");
    let epoch = captured_epoch(&f);
    assert!(
        epoch.fingerprint.is_some(),
        "a measured ledger exists at the stale fingerprint (zero eligible partitions)"
    );
    let v = union_callers(&f, &epoch, "calleeFn");
    assert_row_count_invariant(&v, "callers");
    assert_eq!(v["backend_used"], "sqlite");
    assert_eq!(
        v["fallback_reason"], "LiveGraphStale",
        "today's W-ONE reason"
    );
    assert!(v.get("witness_counts").is_none(), "no union fields — W-ONE");
    for row in rows(&v, "callers") {
        assert!(row.get("witness").is_none());
    }
}

// ── CAPTURE-FAILED (transient 2) ─────────────────────────────────────────────────────────────

#[test]
fn capture_failure_is_retained_for_doctor_and_serving_falls_back() {
    // Half 1 (the capture side): a SQLite error during the ledger build -> nothing stored,
    // capture None, the failure RETAINED on `witness_ledger_build_failure` (doctor-reportable;
    // rendering is M-R3a's). Forced by corrupting the db file AFTER RepoState::open — the
    // connection-per-operation model re-opens it for the build reads.
    let f = test_fixture::build_fixture(false);
    let db_path = f._dir.path().join("repo.db");
    std::fs::write(&db_path, b"not a database").expect("corrupt db");
    assert_eq!(
        callgraph_union_eligibility(&f.state, &f.snapshot_uid),
        None,
        "no valid ledger -> no capture"
    );
    let failure = f.state.witness_ledger_build_failure.read().clone();
    let failure = failure.expect("build failure retained (§4.2 transient 2 substance)");
    assert_eq!(failure.reason, "sqlite_error_during_ledger_walk");

    // Half 2 (the serve side): a request whose capture produced no fingerprint serves pipeline
    // rows through today's channel (`LiveGraphUnavailable` — the ledger genuinely is not
    // available). Driven on a HEALTHY fixture so the pipeline read can serve.
    let g = test_fixture::build_fixture(false);
    let epoch = epoch_with(&g, None);
    let v = union_callers(&g, &epoch, "calleeFn");
    assert_row_count_invariant(&v, "callers");
    assert_eq!(v["backend_used"], "sqlite");
    assert_eq!(v["fallback_reason"], "LiveGraphUnavailable");
    assert!(v.get("witness_counts").is_none());
}

// ── Collision-withheld pairs NEVER serve (§3.5 guard 2 through serving) ──────────────────────

#[test]
fn collision_withheld_pairs_never_serve() {
    // M-R1's guard fixture driven through union serving: the S callee node is a
    // `ScipSynthesizedFallback` whose key byte-equals the pipeline key -> the S pair is WITHHELD
    // from the classification multiset (M-R1 proves `withheld_pairs` + the s_calls exclusion).
    // Iteration 2 (the review-1 §3.6 fix): the fallback-identity endpoint still degrades the
    // projection's envelope to `Partial`, but a per-symbol `Partial` inside W-BOTH now SERVES the
    // union — so the STRUCTURAL barrier carries alone: the ledger's collision-excluded `s_calls`
    // is the assembly's ONLY S source, and the withheld pair is `dual_measured: false` (every
    // projection touching the fallback endpoint is unanswerable), so its P row serves UNMEASURED.
    // Assert the served TRUTH: a union answer with exactly P's row, NO witness claim on it, no
    // `both`, no `semantic`, no S-minted row — the collision serves NOTHING, in either direction.
    let f = test_fixture::build_collision_fixture();
    let epoch = captured_epoch(&f);
    assert!(epoch.fingerprint.is_some(), "measured ledger -> captures");
    let v = union_callers(&f, &epoch, "calleeFn");
    assert_row_count_invariant(&v, "callers");
    assert_eq!(
        v["backend_used"], "union",
        "a per-symbol Partial inside W-BOTH serves (§3.6) — the envelope no longer falls back"
    );
    assert_eq!(v["fallback_reason"], Value::Null);
    let r = rows(&v, "callers");
    assert_eq!(
        r.len(),
        1,
        "exactly P's row; the withheld S instance serves NOTHING"
    );
    for row in &r {
        assert!(
            row.get("witness").is_none(),
            "the pair is measured by NEITHER projection -> no witness claim (unmeasured)"
        );
    }
    assert_eq!(
        witness_counts(&v),
        (0, 0, 0, 1),
        "the P instance counts `unmeasured` — the composition never hides (§3.6-ii)"
    );
    let v2 = union_callees(&f, &epoch, "callerFn");
    assert_row_count_invariant(&v2, "callees");
    assert_eq!(v2["backend_used"], "union");
    let r2 = rows(&v2, "callees");
    assert_eq!(r2.len(), 1);
    for row in &r2 {
        assert!(row.get("witness").is_none());
    }
    assert_eq!(witness_counts(&v2), (0, 0, 0, 1));
}

// ── Per-symbol unanswerability INSIDE W-BOTH (§3.6 — the review-1 required test) ─────────────

#[test]
fn partial_projection_inside_w_both_serves_union_with_unmeasured_counts() {
    // The review-1 required serving test: a Fresh/resident ELIGIBLE TS partition whose current
    // callers/callees projection is `Partial` (per-symbol identity degradation — the fixture's
    // fallback-identity endpoint). The answer must SERVE all available union facts, omit per-row
    // witness claims exactly where no projection measured the pair, and emit nonzero
    // `witness_counts.unmeasured` with the four counts summing to `rows.len()`.
    let f = test_fixture::build_partial_unanswerable_fixture();
    let epoch = captured_epoch(&f);
    assert!(epoch.fingerprint.is_some(), "measured ledger -> captures");

    // PRECONDITION (the reviewer's test spec, asserted not assumed): the anchor's own projection
    // is `Partial` while its partition is Fresh/resident/eligible.
    let target = resolve(&f, "calleeFn");
    {
        let guard = f.state.livegraph.read();
        let lg = guard.as_ref().expect("livegraph resident");
        let env = lg.callers(
            &target.stable_key,
            repo_graph_trust_model::Granularity::CallerDetail,
        );
        assert_eq!(
            env.class(),
            repo_graph_trust_model::AnswerClass::Partial,
            "per-symbol unanswerable: the anchor's OWN projection is Partial"
        );
        assert_eq!(
            env.freshness(),
            repo_graph_trust_model::FreshnessState::Fresh
        );
        assert!(
            env.missing_partitions().is_empty(),
            "no residency degradation"
        );
        let ledger = f.state.witness_ledger.read();
        let cls = ledger
            .as_ref()
            .and_then(|l| l.classification.as_ref())
            .expect("measured classification");
        assert!(
            cls.eligible.contains_key("p"),
            "the partition is W-BOTH-eligible (resident ∧ Fresh)"
        );
    }

    // callers(calleeFn): a MIXED union answer — the clean pair is measured from the OTHER
    // endpoint's projection (syntactic); the fallback-touching pair is measured by neither
    // (unmeasured, NO false row witness).
    let storage = f.state.storage().expect("open storage");
    let p_json: Vec<Value> = storage
        .find_direct_callers(&f.snapshot_uid, &target.stable_key, &["CALLS"])
        .expect("sqlite rows")
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    let v = union_callers(&f, &epoch, "calleeFn");
    assert_row_count_invariant(&v, "callers");
    assert_eq!(v["backend_used"], "union");
    assert_eq!(v["fallback_reason"], Value::Null);
    let r = rows(&v, "callers");
    assert_eq!(
        r.len(),
        2,
        "both P facts serve — facts are never withheld (§3.6-i)"
    );
    let stripped: Vec<Value> = r.iter().cloned().map(strip_witness).collect();
    assert_eq!(stripped, p_json, "union ⊇ P verbatim (order + all fields)");
    let by_key = |key: &str| -> &Value {
        r.iter()
            .find(|row| row["stable_key"] == key)
            .expect("served row for key")
    };
    assert!(
        by_key(&test_fixture::caller_key()).get("witness").is_none(),
        "neither projection measured (callerFn, calleeFn) -> NO witness claim"
    );
    assert_eq!(
        by_key(&test_fixture::clean_fn_key())["witness"],
        "syntactic",
        "measured from cleanFn's Exact callees-projection: S measured, holds no such call"
    );
    let (both, semantic, syntactic, unmeasured) = witness_counts(&v);
    assert_eq!(
        (both, semantic, syntactic, unmeasured),
        (0, 0, 1, 1),
        "nonzero `unmeasured`, 1:1 with the row multiset"
    );
    assert_eq!(
        (both + semantic + syntactic + unmeasured) as usize,
        r.len(),
        "the four counts sum to rows.len() (§5.2)"
    );

    // callees(callerFn): the same §3.6 rule in the OTHER direction — the anchor's projection is
    // Partial (fallback-identity callee), the single pair unmeasured.
    let v2 = union_callees(&f, &epoch, "callerFn");
    assert_row_count_invariant(&v2, "callees");
    assert_eq!(v2["backend_used"], "union");
    assert_eq!(v2["fallback_reason"], Value::Null);
    let r2 = rows(&v2, "callees");
    assert_eq!(r2.len(), 1);
    assert!(r2[0].get("witness").is_none(), "no false row witness");
    assert_eq!(witness_counts(&v2), (0, 0, 0, 1));
}

#[test]
fn unavailable_anchor_in_eligible_partition_serves_union_with_unmeasured_counts() {
    // The review-2 required serving test: a P-only anchor (`ghostFn`) in a Fresh, RESIDENT TS
    // file but ABSENT from the S xref — per-symbol class `Unavailable` while the anchor still
    // belongs to W-BOTH. The answer must serve the union (never today's blanket fallback), with
    // dual-measured rows retaining their class, unmeasured rows carrying no witness, nonzero
    // `witness_counts.unmeasured`, and the four counts summing to `rows.len()`. Both directions.
    let f = test_fixture::build_unavailable_in_w_both_fixture();
    let epoch = captured_epoch(&f);
    assert!(epoch.fingerprint.is_some(), "measured ledger -> captures");

    // PRECONDITION (asserted, not assumed): the anchor's OWN projections are `Unavailable` in
    // BOTH directions, while its FILE's partition state is eligible (resident ∧ Fresh ∧ TS) —
    // exactly the two-axes split the ladder must honor.
    let target = resolve(&f, "ghostFn");
    {
        let guard = f.state.livegraph.read();
        let lg = guard.as_ref().expect("livegraph resident");
        for env_class in [
            lg.callers(
                &target.stable_key,
                repo_graph_trust_model::Granularity::CallerDetail,
            )
            .class(),
            lg.callees(
                &target.stable_key,
                repo_graph_trust_model::Granularity::CallerDetail,
            )
            .class(),
        ] {
            assert_eq!(
                env_class,
                repo_graph_trust_model::AnswerClass::Unavailable,
                "per-symbol unanswerable: the anchor is absent from the S xref"
            );
        }
        let status = lg
            .file_partition_status(target.file.as_deref().expect("pipeline file coordinate"))
            .expect("the anchor's FILE is resident in S");
        assert!(status.fresh, "eligible: Fresh");
        assert!(status.ts_primary, "eligible: TS");
        let ledger = f.state.witness_ledger.read();
        let cls = ledger
            .as_ref()
            .and_then(|l| l.classification.as_ref())
            .expect("measured classification");
        assert!(
            cls.eligible.contains_key("p"),
            "the partition is W-BOTH-eligible (resident ∧ Fresh)"
        );
    }

    // callers(ghostFn): `callerFn` measured from ITS Exact callees-projection (syntactic —
    // dual-measured rows retain their class); `ghostCaller` measured by neither (unmeasured,
    // NO false row witness).
    let storage = f.state.storage().expect("open storage");
    let p_json: Vec<Value> = storage
        .find_direct_callers(&f.snapshot_uid, &target.stable_key, &["CALLS"])
        .expect("sqlite rows")
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    let v = union_callers(&f, &epoch, "ghostFn");
    assert_row_count_invariant(&v, "callers");
    assert_eq!(
        v["backend_used"], "union",
        "an Unavailable anchor INSIDE W-BOTH serves (§3.6) — no blanket fallback (review-2)"
    );
    assert_eq!(v["fallback_reason"], Value::Null);
    let r = rows(&v, "callers");
    assert_eq!(r.len(), 2, "both P facts serve — facts are never withheld");
    let stripped: Vec<Value> = r.iter().cloned().map(strip_witness).collect();
    assert_eq!(stripped, p_json, "union ⊇ P verbatim (order + all fields)");
    let by_key = |rows: &[Value], key: &str| -> Value {
        rows.iter()
            .find(|row| row["stable_key"] == key)
            .expect("served row for key")
            .clone()
    };
    assert_eq!(
        by_key(&r, &test_fixture::caller_key())["witness"],
        "syntactic",
        "measured from callerFn's Exact callees-projection: S measured, holds no such call"
    );
    assert!(
        by_key(&r, &test_fixture::ghost_caller_key())
            .get("witness")
            .is_none(),
        "neither projection measured (ghostCaller, ghostFn) -> NO witness claim"
    );
    let (both, semantic, syntactic, unmeasured) = witness_counts(&v);
    assert_eq!(
        (both, semantic, syntactic, unmeasured),
        (0, 0, 1, 1),
        "nonzero `unmeasured`, 1:1 with the row multiset"
    );
    assert_eq!(
        (both + semantic + syntactic + unmeasured) as usize,
        r.len(),
        "the four counts sum to rows.len() (§5.2)"
    );

    // callees(ghostFn): the SAME Unavailable anchor in the other direction — `calleeFn` measured
    // from ITS Exact callers-projection (syntactic), `ghostTarget` by neither (unmeasured).
    let p2_json: Vec<Value> = storage
        .find_direct_callees(&f.snapshot_uid, &target.stable_key, &["CALLS"])
        .expect("sqlite rows")
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    let v2 = union_callees(&f, &epoch, "ghostFn");
    assert_row_count_invariant(&v2, "callees");
    assert_eq!(v2["backend_used"], "union");
    assert_eq!(v2["fallback_reason"], Value::Null);
    let r2 = rows(&v2, "callees");
    assert_eq!(r2.len(), 2);
    let stripped2: Vec<Value> = r2.iter().cloned().map(strip_witness).collect();
    assert_eq!(
        stripped2, p2_json,
        "union ⊇ P verbatim (order + all fields)"
    );
    assert_eq!(
        by_key(&r2, &test_fixture::callee_key())["witness"],
        "syntactic"
    );
    assert!(by_key(&r2, &test_fixture::ghost_target_key())
        .get("witness")
        .is_none());
    let (b2, se2, sy2, u2) = witness_counts(&v2);
    assert_eq!((b2, se2, sy2, u2), (0, 0, 1, 1));
    assert_eq!((b2 + se2 + sy2 + u2) as usize, r2.len());
}

// ── The ADDED pipeline-only fixture, through serving ─────────────────────────────────────────

#[test]
fn pipeline_only_fixture_serves_syntactic_rows_and_uncovered_answers_fall_back() {
    let f = test_fixture::build_pipeline_only_fixture();
    let epoch = captured_epoch(&f);
    assert!(
        epoch.fingerprint.is_some(),
        "divergent (RED) but measured -> captures under the flag"
    );

    // callees(callerFn): three P rows, ALL dual-measured `syntactic` (boundary + same-partition
    // uncorroborated + endpoint-absent uncorroborated — the amodx-informed shapes); no S rows.
    let v = union_callees(&f, &epoch, "callerFn");
    assert_row_count_invariant(&v, "callees");
    let r = rows(&v, "callees");
    assert_eq!(r.len(), 3);
    for row in &r {
        assert_eq!(
            row["witness"], "syntactic",
            "S measured these projections and corroborates none (row label; sub-classes are \
             ledger/rollup facts, not row fields)"
        );
    }
    assert_eq!(witness_counts(&v), (0, 0, 3, 0));

    // callers(calleeFn): the boundary pair's single P row, `syntactic`.
    let v2 = union_callers(&f, &epoch, "calleeFn");
    assert_row_count_invariant(&v2, "callers");
    let r2 = rows(&v2, "callers");
    assert_eq!(r2.len(), 1);
    assert_eq!(r2[0]["witness"], "syntactic");
    assert_eq!(r2[0]["stable_key"], test_fixture::caller_key());

    // R-1 shape (mixed-repo scoping), retained separately per review-2: an UNCOVERED-language
    // anchor (`rustFn`, class `Unavailable`) whose FILE (`src/r.rs`) is in NO resident partition
    // — `file_partition_status` returns `None`, so this is a genuine W-NONE answer: today's
    // exact fallback path — byte-identical rows, no witness fields, today's reason.
    let v3 = union_callers(&f, &epoch, "rustFn");
    assert_row_count_invariant(&v3, "callers");
    assert_eq!(v3["backend_used"], "sqlite");
    assert_eq!(v3["fallback_reason"], "LiveGraphUnavailable");
    assert!(v3.get("witness_counts").is_none());
    let r3 = rows(&v3, "callers");
    assert_eq!(
        r3.len(),
        2,
        "callerFn + rustCaller — the pipeline facts serve"
    );
    for row in &r3 {
        assert!(row.get("witness").is_none());
    }
}

// ── R-0 shape: no second witness anywhere ⇒ the flag-ON path is byte-identical to today ──────

#[test]
fn lg_less_repo_serves_byte_identical_bytes_with_the_flag_on() {
    // The strict-generalization proof at unit scale (nginx/petclinic's shape): with NO LiveGraph,
    // the flag-ON union path and today's flag-OFF Auto path produce the SAME bytes — the union
    // operator with witness S absent is the identity on witness P (R-0's formal statement).
    let f = test_fixture::build_fixture(false);
    *f.state.livegraph.write() = None;
    let flag_off = flag_off_callers(&f, "calleeFn");
    let fp = callgraph_union_eligibility(&f.state, &f.snapshot_uid);
    assert_eq!(fp, None, "no LiveGraph -> no capture");
    let epoch = epoch_with(&f, fp);
    let flag_on = union_callers(&f, &epoch, "calleeFn");
    assert_eq!(flag_on, flag_off, "byte-identical (R-0)");
}

// ── W-B epoch: the pin re-check is the EPOCH-MOVED test above; eviction unchanged (M-R1) ─────

#[test]
fn faithful_mirror_union_serve_tags_every_row_both() {
    // GREEN graph (LG == SQLite): every P row is fully corroborated -> `both` on each, no minted
    // rows, exact counts. (The flag-OFF GREEN path — LG placeholder rows — is byte-frozen and
    // untouched; this asserts the flag-ON shape only.)
    let f = test_fixture::build_fixture(false);
    let epoch = captured_epoch(&f);
    let v = union_callers(&f, &epoch, "calleeFn");
    assert_row_count_invariant(&v, "callers");
    let r = rows(&v, "callers");
    assert_eq!(r.len(), 1);
    assert_eq!(r[0]["witness"], "both");
    assert_eq!(
        r[0]["name"], "callerFn",
        "P row fields verbatim (real name)"
    );
    assert_eq!(witness_counts(&v), (1, 0, 0, 0));
}
