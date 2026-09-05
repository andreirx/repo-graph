//! Unit tests for the seed-tier renderer (`seed_render.rs`). Extracted to a sibling test
//! source (review-2 item 4: keep the production file under the 500-line guardrail) and
//! included via `#[path]` — the same pattern as `classify_tests.rs`. `super` resolves to
//! the `seed_render` module, so `use super::*` reaches its private renderer helpers.

use super::*;
use crate::commands::find::test_fixtures::{
    candidate_with_score, empty_facts, well_formed_candidate,
};
use crate::commands::find::FactTierOutcome;
use serde_json::json;

#[test]
fn all_seeds_below_floor_abstains_with_best_score() {
    // FIND-RANK-1 §2.3: 10 sub-floor seeds (the lowest-no-home shape) → the honest
    // abstain with the best score, NOT 10 rows of hearsay. Scores are below the
    // potion-calibrated floor (0.30); the best is 0.136.
    let candidates: Vec<serde_json::Value> = (0..10)
        .map(|i| candidate_with_score(&format!("k{i}"), 0.10 + f64::from(i) * 0.004))
        .collect();
    let mut out = String::new();
    // Facts established a MISS → the §2.4 capability close is appended to the abstain.
    render_seed_tier(
        &json!({"seeds_available": true, "candidates": candidates}),
        FactTierOutcome::EstablishedMiss,
        None,
        &mut out,
    );
    assert!(
        out.contains(
            "no candidates above the minimum similarity 0.30 (best: 0.14) — nothing matched."
        ),
        "abstain states capability, not repo absence, in noise-tail-cutoff wording: {out}"
    );
    // SEED-CHUNK-2 §2.3.3: the `--text` referral renders as its own line whenever
    // seeds serve — including on the abstain path.
    assert!(
        out.contains("for exact text, comments, or expressions: rmap find --text"),
        "the --text referral renders on the abstain path: {out}"
    );
    // FIND-GREP-1 §4: the false repo-absence sentence is RETIRED everywhere.
    assert!(
        !out.contains("distinct home"),
        "retired false repo-absence sentence must not render: {out}"
    );
    // Not one sub-floor candidate rendered.
    assert!(
        !out.contains("src/x.ts"),
        "no sub-floor rows rendered: {out}"
    );
}

#[test]
fn seeds_above_floor_render_and_sub_floor_are_dropped_without_abstain() {
    // A mix: one above-floor seed renders; a below-floor one is silently dropped and
    // the abstain line does NOT appear (the tier answered).
    let mut out = String::new();
    render_seed_tier(
        &json!({
            "seeds_available": true,
            "candidates": [
                candidate_with_score("above", 0.72),
                candidate_with_score("below", 0.22),
            ],
        }),
        FactTierOutcome::EstablishedMiss,
        None,
        &mut out,
    );
    assert!(
        out.contains("score 0.72, embedding"),
        "above-floor rendered: {out}"
    );
    assert!(
        !out.contains("no candidates above the minimum similarity"),
        "no abstain when a seed cleared the floor: {out}"
    );
    // SEED-CHUNK-2 §2.3.3: the referral renders even when a candidate served — "not
    // only on empty tiers" (the `find fsync` defect this fixes).
    assert!(
        out.contains("for exact text, comments, or expressions: rmap find --text"),
        "the --text referral renders beside served candidates: {out}"
    );
}

#[test]
fn text_referral_uses_the_actual_query_when_present() {
    // SEED-CHUNK-2 §2.3.3: the referral is a copy-paste-runnable command with the
    // real query when the DTO carries one. review-1 item 3: a shell-safe query
    // (alphanumerics only) renders BARE — no quotes needed to run it verbatim.
    let mut out = String::new();
    render_seed_tier(
        &json!({
            "seeds_available": true,
            "query": "fsync",
            "candidates": [candidate_with_score("above", 0.72)],
        }),
        FactTierOutcome::MissNotEstablished,
        None,
        &mut out,
    );
    assert!(
        out.contains("rmap find --text fsync\n"),
        "referral runs the actual (shell-safe) query bare: {out}"
    );
}

#[test]
fn text_referral_shell_quotes_a_query_with_spaces_and_quotes() {
    // review-1 item 3: a query containing spaces / double-quotes / shell
    // metacharacters must render a RUNNABLE, non-injecting copy-paste line — the
    // prior raw `"{q}"` interpolation produced a broken (and injectable) command.
    // POSIX single-quoting wraps the whole argument and escapes embedded single
    // quotes; a double quote inside single quotes is literal and safe.
    let mut out = String::new();
    render_seed_tier(
        &json!({
            "seeds_available": true,
            "query": "crash \"recovery\"; rm -rf /",
            "candidates": [candidate_with_score("above", 0.72)],
        }),
        FactTierOutcome::MissNotEstablished,
        None,
        &mut out,
    );
    // The whole query is one single-quoted argument (the metacharacters `;`, `"`,
    // spaces are all inside the quotes, so the shell never interprets them).
    assert!(
        out.contains("rmap find --text 'crash \"recovery\"; rm -rf /'\n"),
        "the query is POSIX single-quoted as ONE safe argument: {out}"
    );
    // The referral line does NOT terminate the intended argument early — there is no
    // bare (unquoted) `;` that the shell would treat as a command separator.
    let referral_line = out
        .lines()
        .find(|l| l.contains("rmap find --text"))
        .expect("referral line present");
    assert!(
        !referral_line.contains("--text 'crash \"recovery\"';"),
        "no early-terminated quoting that leaks a metacharacter: {referral_line}"
    );
}

#[test]
fn text_referral_placeholder_is_a_bare_fill_in_marker() {
    // review-1 item 3: with no query (a malformed response), the referral keeps the
    // `<pattern>` placeholder as a BARE human fill-in marker — not a fabricated query
    // and not a spuriously-quoted literal to run.
    let mut out = String::new();
    render_seed_tier(
        &json!({
            "seeds_available": true,
            "candidates": [candidate_with_score("above", 0.72)],
        }),
        FactTierOutcome::MissNotEstablished,
        None,
        &mut out,
    );
    assert!(
        out.contains("rmap find --text <pattern>\n"),
        "placeholder stays a bare fill-in marker: {out}"
    );
}

#[test]
fn decl_candidate_renders_the_decl_label() {
    // SEED-CHUNK-2 §2.2: a declaration chunk renders `(decl)`; an impl does not.
    let mut out = String::new();
    let decl = json!({
        "stable_key": "k1", "path": "db/db_impl.h", "line": 113,
        "qualified_name": "leveldb::DBImpl::Recover", "is_test": false, "is_decl": true,
        "score": 0.45, "source": "embedding", "model_id": "minishlab/potion-code-16M-v2",
        "module": {"owning": "db"},
        "next": {"cmd": "explain", "args": ["k1"], "cwd": "/repo"}
    });
    render_seed_tier(
        &json!({"seeds_available": true, "query": "recover", "candidates": [decl]}),
        FactTierOutcome::MissNotEstablished,
        None,
        &mut out,
    );
    assert!(
        out.contains("(decl)"),
        "declaration chunk labeled (decl): {out}"
    );
}

#[test]
fn text_referral_absent_when_seeds_do_not_serve() {
    // SEED-CHUNK-2 §2.3.3: "whenever seeds SERVE" — an unavailable tier is not
    // serving, so no referral (it would advertise a move beside an error, not a result).
    let mut out = String::new();
    render_seed_tier(
        &json!({
            "seeds_available": false,
            "seeds_unavailable_reason": "no seed vectors yet",
            "query": "fsync",
        }),
        FactTierOutcome::MissNotEstablished,
        None,
        &mut out,
    );
    assert!(
        !out.contains("rmap find --text"),
        "no referral when the tier is unavailable: {out}"
    );
}

#[test]
fn seed_below_floor_with_missing_score_is_still_surfaced_not_swallowed() {
    // A candidate with NO score cannot be floor-filtered — it must still be VALIDATED
    // (and, failing, COUNTED as unreadable on one honest line), never silently dropped
    // by the floor (STANDING HONESTY RULE 1). CURSOR-ROUNDTRIP-1 (§2.2): the shared
    // renderer counts + states it, never a per-row `(malformed candidate: …)` placeholder.
    let mut out = String::new();
    render_seed_tier(
        &json!({"seeds_available": true, "candidates": [{"path": "src/x.ts"}]}),
        FactTierOutcome::EstablishedMiss,
        None,
        &mut out,
    );
    assert!(
        out.contains("1 candidate unreadable: missing required field"),
        "counted + stated on one line: {out}"
    );
    assert!(
        !out.contains("(malformed candidate"),
        "no per-row placeholder (RULE 1): {out}"
    );
}

#[test]
fn well_formed_candidate_renders_with_validated_label() {
    let mut out = String::new();
    let result = json!({
        "seeds_available": true,
        "candidates": [well_formed_candidate(json!("embedding"))],
    });
    render_seed_tier(&result, FactTierOutcome::EstablishedMiss, None, &mut out);
    assert!(
        out.contains("score 0.71, embedding, model nomic-embed-text-v1.5"),
        "candidate label: {out}"
    );
    assert!(out.contains(", module backend/auth"), "module hint: {out}");
}

#[test]
fn chunk_seed_renders_path_line_anchor_qualified_name_and_test_label() {
    // SEED-CHUNK-1: a per-SYMBOL chunk seed renders `path:line` + qualified name;
    // a test-classified chunk is labeled `[test]` (production above test, spec §5).
    let mut out = String::new();
    let prod = json!({
        "stable_key": "k1", "path": "db/db_impl.cc", "line": 415,
        "qualified_name": "leveldb::DBImpl::RecoverLogFile", "is_test": false,
        "score": 0.71, "source": "embedding", "model_id": "minishlab/potion-code-16M-v2",
        "module": {"owning": "db"},
        "next": {"cmd": "explain", "args": ["k1"], "cwd": "/repo"}
    });
    let test = json!({
        "stable_key": "k2", "path": "db/recovery_test.cc", "line": 30,
        "qualified_name": "leveldb::RecoveryTest::OpenWithStatus", "is_test": true,
        "score": 0.65, "source": "embedding", "model_id": "minishlab/potion-code-16M-v2",
        "module": {"owning": "db"},
        "next": {"cmd": "explain", "args": ["k2"], "cwd": "/repo"}
    });
    render_seed_tier(
        &json!({"seeds_available": true, "candidates": [prod, test]}),
        FactTierOutcome::EstablishedMiss,
        None,
        &mut out,
    );
    assert!(
        out.contains("db/db_impl.cc:415  leveldb::DBImpl::RecoverLogFile  (score 0.71"),
        "production chunk anchor + qualified name: {out}"
    );
    assert!(
        out.contains("db/recovery_test.cc:30  leveldb::RecoveryTest::OpenWithStatus  (score 0.65")
            && out.contains("[test]"),
        "test chunk is anchored AND labeled [test]: {out}"
    );
    // SEED-CHUNK-2 §2.1: both partitions non-empty → the partition header renders,
    // BELOW the production row and ABOVE the test row (the moat boundary is explicit).
    let header = "  tests (ranked below production, labeled [test]):\n";
    assert!(out.contains(header), "partition header renders: {out}");
    let prod_at = out.find("db/db_impl.cc:415").expect("prod row present");
    let header_at = out.find(header).expect("header present");
    let test_at = out
        .find("db/recovery_test.cc:30")
        .expect("test row present");
    assert!(
        prod_at < header_at && header_at < test_at,
        "header sits between the production block and the test block: {out}"
    );
}

#[test]
fn partition_header_absent_when_only_production_candidates() {
    // SEED-CHUNK-2 §2.1: the header renders ONLY when BOTH partitions are non-empty.
    // An all-production list is unambiguous on its own — no header.
    let prod = |k: &str, line: i64, score: f64| {
        json!({
            "stable_key": k, "path": "db/db_impl.cc", "line": line,
            "qualified_name": format!("leveldb::DBImpl::{k}"), "is_test": false,
            "score": score, "source": "embedding",
            "model_id": "minishlab/potion-code-16M-v2", "module": {"owning": "db"},
            "next": {"cmd": "explain", "args": [k], "cwd": "/repo"}
        })
    };
    let mut out = String::new();
    render_seed_tier(
        &json!({
            "seeds_available": true,
            "candidates": [prod("Recover", 292, 0.72), prod("Write", 1206, 0.61)],
        }),
        FactTierOutcome::MissNotEstablished,
        None,
        &mut out,
    );
    assert!(
        !out.contains("tests (ranked below production"),
        "no partition header with a single (production) partition: {out}"
    );
    assert!(
        !out.contains("[test]"),
        "no test label on an all-production list: {out}"
    );
}

#[test]
fn partition_header_absent_when_only_test_candidates() {
    // SEED-CHUNK-2 §2.1: an all-test list carries per-row `[test]` labels and needs no
    // header (there is no production block to separate it from).
    let test = json!({
        "stable_key": "t1", "path": "db/recovery_test.cc", "line": 30,
        "qualified_name": "leveldb::RecoveryTest::OpenWithStatus", "is_test": true,
        "score": 0.65, "source": "embedding", "model_id": "minishlab/potion-code-16M-v2",
        "module": {"owning": "db"},
        "next": {"cmd": "explain", "args": ["t1"], "cwd": "/repo"}
    });
    let mut out = String::new();
    render_seed_tier(
        &json!({"seeds_available": true, "candidates": [test]}),
        FactTierOutcome::MissNotEstablished,
        None,
        &mut out,
    );
    assert!(out.contains("[test]"), "test row still labeled: {out}");
    assert!(
        !out.contains("tests (ranked below production"),
        "no partition header with a single (test) partition: {out}"
    );
}

#[test]
fn missing_is_test_renders_unknown_marker_never_blank_like_production() {
    // review-1 gap d: a candidate whose `is_test` is ABSENT must render an explicit
    // unknown marker — never blank (which reads as unlabeled production). Unknown
    // classification is never invisible (STANDING HONESTY).
    let mut out = String::new();
    let cand = json!({
        "stable_key": "k1", "path": "db/db_impl.cc", "line": 42,
        "qualified_name": "leveldb::DBImpl::Foo",
        // NO is_test key
        "score": 0.71, "source": "embedding", "model_id": "minishlab/potion-code-16M-v2",
        "module": {"owning": "db"},
        "next": {"cmd": "explain", "args": ["k1"], "cwd": "/repo"}
    });
    render_seed_tier(
        &json!({"seeds_available": true, "candidates": [cand]}),
        FactTierOutcome::EstablishedMiss,
        None,
        &mut out,
    );
    assert!(
        out.contains("[is_test unknown]"),
        "absent is_test surfaces an explicit unknown marker: {out}"
    );
    assert!(
        !out.contains("[test]") || out.contains("[is_test unknown]"),
        "unknown is not silently treated as production: {out}"
    );
}

#[test]
fn non_embedding_seed_source_is_counted_never_relabeled() {
    // A non-`embedding` source is COUNTED unreadable with the offending source named,
    // never relabeled as an embedding hint, never a per-row placeholder (RULE 1).
    let _ = empty_facts(); // fixture parity with the other tiers' tests.
    let mut out = String::new();
    let result = json!({
        "seeds_available": true,
        "candidates": [well_formed_candidate(json!("lexical"))],
    });
    render_seed_tier(&result, FactTierOutcome::EstablishedMiss, None, &mut out);
    assert!(out.contains("unreadable"), "surfaced unreadable: {out}");
    assert!(out.contains("\"lexical\""), "names offending source: {out}");
    assert!(!out.contains("0.71, embedding"), "not relabeled: {out}");
    assert!(
        !out.contains("(malformed candidate"),
        "no per-row placeholder: {out}"
    );
}

#[test]
fn seeds_available_missing_is_malformed_never_defaulted() {
    let mut out = String::new();
    render_seed_tier(
        &json!({"candidates": []}),
        FactTierOutcome::EstablishedMiss,
        None,
        &mut out,
    );
    assert!(
        out.contains("malformed find response: seeds_available missing or not a bool"),
        "{out}"
    );
}

#[test]
fn unavailable_without_reason_is_malformed() {
    let mut out = String::new();
    render_seed_tier(
        &json!({"seeds_available": false}),
        FactTierOutcome::EstablishedMiss,
        None,
        &mut out,
    );
    assert!(
        out.contains("malformed find response: seeds unavailable but no reason given"),
        "{out}"
    );
}

// ── ECONOMY-2 §1: seed-row cursor diet ──────────────────────────────────────

#[test]
fn in_root_composable_seed_row_drops_cursor_and_shows_kind() {
    // ECONOMY-2 (§2.1, ruling economy_2_cursor_metric): with the header uid present, an
    // IN-ROOT seed row whose OWN path + qualified_name reassemble the runnable short cursor
    // (`<path>#<qualified_name>:SYMBOL:<KIND>`) is CURSOR-COMPOSABLE — the ONE pattern header
    // `find` prints covers it, so its per-row `→ rmap explain …` line is DROPPED entirely and
    // the row shows `[KIND]` instead. This is how the LITERAL ≤15% target is met by design.
    let mut out = String::new();
    let result = json!({
        "seeds_available": true,
        "candidates": [{
            "path": "db/db_impl.cc", "line": 42, "qualified_name": "Recover",
            "stable_key": "leveldb-uid:db/db_impl.cc#Recover:SYMBOL:FUNCTION",
            "is_test": false, "is_decl": false,
            "score": 0.71, "source": "embedding", "model_id": "m",
            "module": {"owning": "db"},
            "next": {"cwd": "/some/abs/path/leveldb"}
        }],
    });
    render_seed_tier(
        &result,
        FactTierOutcome::EstablishedMiss,
        Some("leveldb-uid"),
        &mut out,
    );
    // No per-row cursor line at all — the header covers it.
    assert!(
        !out.contains("→ rmap explain"),
        "composable in-root seed row drops its per-row cursor line: {out}"
    );
    assert!(
        !out.contains("cd /some/abs/path/leveldb"),
        "no cd wrap on a composable in-root seed row: {out}"
    );
    // The row shows the KIND inline, and the identity from which the header pattern
    // reassembles the cursor (`db/db_impl.cc#Recover:SYMBOL:FUNCTION`).
    assert!(
        out.contains("db/db_impl.cc:42  Recover") && out.contains("[FUNCTION]"),
        "row carries path:line, qualified_name, and [KIND]: {out}"
    );
    // PROVE the runnable pattern: the visible fields reassemble the uid-stripped suffix the
    // JSON `cursor_raw` carries and the daemon alias resolves.
    assert_eq!(
        crate::presentation::seed::composable_cursor_kind(
            Some("leveldb-uid"),
            "leveldb-uid:db/db_impl.cc#Recover:SYMBOL:FUNCTION",
            "db/db_impl.cc",
            "Recover",
        )
        .as_deref(),
        Some("FUNCTION"),
        "row fields reassemble the cursor suffix exactly"
    );
}

#[test]
fn out_of_root_seed_cursor_keeps_full_cd_form() {
    // A seed whose `stable_key` does NOT carry THIS repo's `<uid>:` prefix (a foreign /
    // old-daemon key) keeps the full self-contained `(cd <cwd> && rmap explain <key>)`
    // form even when a header uid exists — the short form would not resolve there.
    let mut out = String::new();
    let result = json!({
        "seeds_available": true,
        "candidates": [{
            "path": "x.ts", "line": 1, "qualified_name": "f",
            "stable_key": "OTHER-uid:x.ts#f:SYMBOL:FUNCTION",
            "is_test": false, "is_decl": false,
            "score": 0.71, "source": "embedding", "model_id": "m",
            "module": {"owning": "x"},
            "next": {"cwd": "/other/repo"}
        }],
    });
    render_seed_tier(
        &result,
        FactTierOutcome::EstablishedMiss,
        Some("leveldb-uid"),
        &mut out,
    );
    assert!(
        out.contains("→ (cd /other/repo && rmap explain 'OTHER-uid:x.ts#f:SYMBOL:FUNCTION')\n"),
        "out-of-root seed keeps the full self-contained cursor, `#`-bearing key quoted: {out}"
    );
}

#[test]
fn seed_cursor_stays_full_without_a_header_uid() {
    // No header uid (`None` — degraded / Group-B path) → the full `cd … &&` cursor, never
    // a truncated non-runnable short form. Byte-identical to pre-ECONOMY-2.
    let mut out = String::new();
    let result = json!({
        "seeds_available": true,
        "candidates": [{
            "path": "db/db_impl.cc", "line": 42, "qualified_name": "Recover",
            "stable_key": "leveldb-uid:db/db_impl.cc#Recover:SYMBOL:FUNCTION",
            "is_test": false, "is_decl": false,
            "score": 0.71, "source": "embedding", "model_id": "m",
            "module": {"owning": "db"},
            "next": {"cwd": "/some/abs/path/leveldb"}
        }],
    });
    render_seed_tier(&result, FactTierOutcome::EstablishedMiss, None, &mut out);
    assert!(
        out.contains(
            "→ (cd /some/abs/path/leveldb && rmap explain 'leveldb-uid:db/db_impl.cc#Recover:SYMBOL:FUNCTION')\n"
        ),
        "full cursor without a header uid, `#`-bearing key quoted: {out}"
    );
}
