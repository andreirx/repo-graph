//! Tests for the `find` command entry + tier orchestrator (`super`). Relocated out of
//! `find.rs` per the ≤500-line structural guardrail (FIND-GREP-1 review-2 finding 4) —
//! a test-only child module, NOT a runtime abstraction: `super::*` still reaches the
//! parent's private orchestrator items (`render_find_human`, `diet_eligibility`, …).

use super::*;
use crate::commands::find::test_fixtures::{
    candidate_with_score, empty_facts, well_formed_candidate,
};
use serde_json::json;

#[test]
fn facts_render_above_seeds_with_class_and_command_labels() {
    let result = json!({
        "query": "bnr",
        "facts": [
            {"fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
             "hits": [{"display": "bnrService", "path": "src/bnr.ts", "key": "k1", "next": "explain k1"}],
             "matched": 1, "matched_is_floor": false},
            {"fact_class": "http-surface", "render_command": "boundaries list", "certainty": "inferred",
             "hits": [{"display": "provider GET /api/offers", "path": "src/offer.ts", "next": "boundaries list"}],
             "matched": 1, "matched_is_floor": false},
            {"fact_class": "file", "render_command": "explain", "certainty": "extracted", "hits": [], "matched": 0, "matched_is_floor": false},
            {"fact_class": "module", "render_command": "map --dry-run", "certainty": "inferred", "hits": [], "matched": 0, "matched_is_floor": false},
            {"fact_class": "dependency", "render_command": "deps list", "certainty": "extracted", "hits": [], "matched": 0, "matched_is_floor": false},
            {"fact_class": "framework", "render_command": "inferences list", "certainty": "hint", "hits": [], "matched": 0, "matched_is_floor": false},
            {"fact_class": "boundary", "certainty": "governance", "hits": [], "matched": 0, "matched_is_floor": false}
        ],
        "seeds_available": true,
        "summary": "ranked guesses for \"bnr\" (embedding similarity — not facts)",
        "candidates": [well_formed_candidate(json!("embedding"))],
    });
    let out = render_find_human(&result, false);
    // Facts tier appears BEFORE the seed tier.
    let facts_pos = out
        .find("[symbol · extracted → rmap explain]")
        .expect("symbol label with certainty");
    let seed_pos = out.find("Semantic seeds").expect("seed header");
    assert!(facts_pos < seed_pos, "facts render above seeds:\n{out}");
    assert!(
        out.contains("bnrService  — src/bnr.ts"),
        "symbol hit: {out}"
    );
    // The runnable per-hit next command is rendered (review-1 item 1).
    assert!(
        out.contains("→ rmap explain k1"),
        "runnable per-hit next command: {out}"
    );
    assert!(
        out.contains("[http-surface · inferred → rmap boundaries list]"),
        "http label with certainty: {out}"
    );
    assert!(
        out.contains("provider GET /api/offers  — src/offer.ts"),
        "route hit: {out}"
    );
    // Seed candidate still rendered below, with its validated label.
    assert!(
        out.contains("score 0.71, embedding, model nomic-embed-text-v1.5"),
        "seed candidate below: {out}"
    );
}

#[test]
fn cursor_diet_applies_pattern_header_and_drops_composable_cursor_lines() {
    // ECONOMY-2 (§2.1, ruling economy_2_cursor_metric): CURSOR-COMPOSABLE symbol rows
    // (their Known path + display reassemble `<path>#<display>:SYMBOL:<KIND>`) → the ONE
    // pattern header prints AND every composable row's per-row `→ rmap explain …` line is
    // DROPPED, the row showing `[KIND]` instead. This is the whole-cursor-line elision that
    // meets the LITERAL ≤15% target by design. Keys carry the real `#` format. (This case
    // exercises three composable rows; the single-row case is covered separately.)
    let uid = "repo_01m1kvv00zgrtr3t23xrfe6veg";
    let hit = |name: &str, path: &str| {
        let key = format!("{uid}:{path}#{name}:SYMBOL:FUNCTION");
        json!({"display": name, "path": path, "key": key.clone(),
               "next": format!("explain '{key}'")})
    };
    let mut facts = empty_facts();
    facts[0] = json!({"fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": [hit("alpha", "src/a.ts"), hit("beta", "src/b.ts"), hit("gamma", "src/c.ts")],
        "matched": 3, "matched_is_floor": false});
    let result = json!({
        "query": "sym", "repo_uid": uid, "facts": facts,
        "seeds_available": true, "candidates": [],
    });
    let out = render_find_human(&result, true);
    // The ONE pattern header prints (stating the composition, not the uid).
    assert!(
        out.contains(
            "→ explain any row below: rmap explain '<path>#<qualified_name>:SYMBOL:<KIND>'"
        ),
        "pattern header prints when rows are composable: {out}"
    );
    // No per-row cursor lines — the header covers every composable row. (A per-row cursor is
    // an indented `→ rmap explain <arg>` line; the class label `[symbol · … → rmap explain]`
    // and the pattern header are NOT per-row cursors, so match the row-line shape precisely.)
    assert!(
        !out.lines()
            .any(|l| l.trim_start().starts_with("→ rmap explain")),
        "composable rows drop their per-row cursor lines: {out}"
    );
    // Each row shows its identity + [KIND], from which the pattern reassembles the cursor.
    assert!(
        out.contains("alpha  — src/a.ts  [FUNCTION]"),
        "row carries display, path, and [KIND]: {out}"
    );
    // The uid is never restated per row.
    assert!(
        !out.contains("explain 'repo_01m1kvv00zgrtr3t23xrfe6veg:"),
        "the uid is not restated per row: {out}"
    );
}

#[test]
fn cursor_diet_applies_to_a_single_composable_row_per_contract() {
    // ECONOMY-2 §2.1 (review-0 finding 1): an explicit per-row cursor is permitted ONLY where
    // the row CANNOT compose one. A single composable symbol row therefore takes the diet like
    // any other — the ONE pattern header prints and the row DROPS its per-row `→ rmap explain …`
    // line, showing `[KIND]` instead. The prior `≥2` byte-economy gate that kept a per-row
    // cursor for a lone row was a contract breach; the contract governs over a per-output byte
    // count for the single-row case.
    let uid = "repo_01m1kvv00zgrtr3t23xrfe6veg";
    let key = format!("{uid}:src/x.rs#witness_epoch:SYMBOL:FUNCTION");
    let mut facts = empty_facts();
    facts[0] = json!({"fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": [{"display": "witness_epoch", "path": "src/x.rs", "key": key,
                  "next": format!("explain '{key}'")}],
        "matched": 1, "matched_is_floor": false});
    let result = json!({
        "query": "witness_epoch", "repo_uid": uid, "facts": facts,
        "seeds_available": true, "candidates": [],
    });
    let out = render_find_human(&result, true);
    // The ONE pattern header prints for the lone composable row.
    assert!(
        out.contains(
            "→ explain any row below: rmap explain '<path>#<qualified_name>:SYMBOL:<KIND>'"
        ),
        "pattern header prints for a single composable row: {out}"
    );
    // The row drops its per-row cursor line (an indented `→ rmap explain <arg>` line).
    assert!(
        !out.lines()
            .any(|l| l.trim_start().starts_with("→ rmap explain")),
        "the single composable row drops its per-row cursor line: {out}"
    );
    // The row shows its identity + [KIND], from which the pattern reassembles the cursor.
    assert!(
        out.contains("witness_epoch  — src/x.rs  [FUNCTION]"),
        "row carries display, path, and [KIND]: {out}"
    );
    // The uid is never restated per row.
    assert!(
        !out.contains("explain 'repo_01m1kvv00zgrtr3t23xrfe6veg:"),
        "the uid is not restated per row: {out}"
    );
}

#[test]
fn malformed_top_level_facts_withholds_diet_never_fabricates_header_or_cursor() {
    // review-1 regression: a `facts` payload that is NOT the ratified array (here an
    // object) cannot be classified for the cursor diet. The diet is WITHHELD — NO
    // header line, NO relative cursor selected — and the malformed shape is surfaced
    // honestly by the facts renderer, never silently classified as "0 dietable rows".
    let uid = "repo_01m1kvv00zgrtr3t23xrfe6veg";
    let result = json!({
        "query": "sym", "repo_uid": uid,
        "facts": {"not": "an array"},
        "seeds_available": true, "candidates": [],
    });
    let out = render_find_human(&result, true);
    assert!(
        !out.contains("repo-uid "),
        "no diet header on malformed facts: {out}"
    );
    assert!(
        out.contains("malformed find response: facts missing or not a list"),
        "malformed facts surfaced honestly: {out}"
    );
}

#[test]
fn malformed_group_hits_withholds_diet_keeps_full_self_contained_cursors() {
    // review-1 regression: three uid-prefixed symbol rows WOULD amortize the header,
    // but a second group's `hits` is a non-list. The old classifier silently discarded
    // that group (filter_map) and applied the diet off the survivors; the checked
    // traversal short-circuits to Malformed, so the diet is WITHHELD ENTIRELY — every
    // symbol cursor stays FULL and self-contained (uid restated, still runnable), no
    // header, and the corrupt group is surfaced by the renderer.
    let uid = "repo_01m1kvv00zgrtr3t23xrfe6veg";
    let hit = |k: &str| {
        json!({"display": "sym", "path": "src/x.ts", "key": format!("{uid}:{k}"),
               "next": format!("explain '{uid}:{k}'")})
    };
    let mut facts = empty_facts();
    facts[0] = json!({"fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": [hit("a#a:SYMBOL:FUNCTION"), hit("b#b:SYMBOL:FUNCTION"), hit("c#c:SYMBOL:FUNCTION")],
        "matched": 3, "matched_is_floor": false});
    // A malformed second group: `hits` present but not a list.
    facts[1] = json!({"fact_class": "file", "render_command": "explain", "certainty": "extracted",
        "hits": {"not": "a list"}, "matched": 0, "matched_is_floor": false});
    let result = json!({
        "query": "sym", "repo_uid": uid, "facts": facts,
        "seeds_available": true, "candidates": [],
    });
    let out = render_find_human(&result, true);
    assert!(
        !out.contains("repo-uid "),
        "diet withheld — no header — on a malformed group: {out}"
    );
    assert!(
        out.contains("→ rmap explain 'repo_01m1kvv00zgrtr3t23xrfe6veg:a#a:SYMBOL:FUNCTION'\n"),
        "symbol cursor stays full and self-contained (no relative cursor fabricated): {out}"
    );
    assert!(
        out.contains("malformed fact group: hits missing or not a list"),
        "malformed group surfaced honestly: {out}"
    );
}

#[test]
fn endpoint_down_renders_facts_and_seed_unavailable_with_reason() {
    let mut facts = empty_facts();
    facts[0] = json!({
        "fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": [{"display": "bnrService", "path": "src/bnr.ts", "key": "k1", "next": "explain k1"}],
        "matched": 1, "matched_is_floor": false
    });
    let result = json!({
        "query": "bnr", "facts": facts,
        "seeds_available": false,
        "seeds_unavailable_reason": "no local embedding model reachable; seeding is optional, resolution is unaffected",
        "summary": "no local embedding model reachable — semantic hints unavailable (find is optional)",
        "candidates": [],
    });
    let out = render_find_human(&result, false);
    // Facts intact.
    assert!(
        out.contains("bnrService  — src/bnr.ts"),
        "facts intact: {out}"
    );
    // Seeds explicitly unavailable WITH reason.
    assert!(
        out.contains("semantic seeds unavailable (no local embedding model reachable"),
        "seed unavailable with reason: {out}"
    );
}

#[test]
fn exact_mode_omits_seed_section_entirely() {
    let result = json!({
        "query": "bnr", "facts": empty_facts(),
        "seeds_available": false,
        "seeds_unavailable_reason": "not consulted (--exact — facts only)",
        "candidates": [],
    });
    let out = render_find_human(&result, true);
    assert!(
        !out.contains("Semantic seeds"),
        "no seed section in --exact: {out}"
    );
    assert!(
        out.contains("Facts (deterministic lexical match over the indexed tables"),
        "facts present: {out}"
    );
}

#[test]
fn honest_empty_names_searched_classes() {
    let result = json!({
        "query": "zzz", "facts": empty_facts(),
        "seeds_available": true, "candidates": [],
    });
    let out = render_find_human(&result, false);
    assert!(
        out.contains(
            "no matches: symbol, file, module, http-surface, dependency, framework, boundary"
        ),
        "honest empty names searched classes: {out}"
    );
    assert!(
        out.contains("(no area scored above zero)"),
        "seed empty stated: {out}"
    );
}

#[test]
fn query_missing_is_malformed_never_empty_echo() {
    let result = json!({"facts": empty_facts(), "seeds_available": true, "candidates": []});
    let out = render_find_human(&result, false);
    assert!(
        out.contains("malformed find response: query missing or not a string"),
        "missing query surfaced: {out}"
    );
}

#[test]
fn fact_hit_with_only_subfloor_seeds_does_not_claim_nothing_matched() {
    // review-1 (§2.4): facts MATCHED (a symbol hit) but every seed is sub-floor. The
    // seed tier must still abstain HONESTLY about the seed tier ("no candidates above
    // the minimum similarity") but must NOT append the capability close — "nothing matched"
    // would be FALSE when a fact class matched.
    let mut facts = empty_facts();
    facts[0] = json!({"fact_class": "symbol", "render_command": "explain", "certainty": "extracted",
        "hits": [{"display": "bnrService", "path": "src/bnr.ts", "key": "k1", "next": "explain k1"}],
        "matched": 1, "matched_is_floor": false});
    let result = json!({
        "query": "bnr", "facts": facts,
        "seeds_available": true,
        "candidates": [candidate_with_score("k0", 0.22)],
    });
    let out = render_find_human(&result, false);
    assert!(
        out.contains("bnrService  — src/bnr.ts"),
        "fact hit rendered: {out}"
    );
    assert!(
        out.contains("no candidates above the minimum similarity"),
        "seed abstain still renders honestly: {out}"
    );
    assert!(
        !out.contains("nothing matched"),
        "no false 'nothing matched' when a fact class matched: {out}"
    );
    // SEED-CHUNK-2 §2.3.3: the `--text` referral renders whenever seeds SERVE — even when
    // a fact class matched. It is NOT the "nothing matched" capability close (that stays
    // withheld above); it is the always-on "where exact text is searched" line.
    assert!(
        out.contains("for exact text, comments, or expressions: rmap find --text"),
        "the --text referral renders whenever seeds serve, independent of the facts tier: {out}"
    );
}

#[test]
fn fact_miss_with_only_subfloor_seeds_routes_to_find_text() {
    // review-1 (§2.4): facts MISSED (all seven classes empty, envelope complete) and
    // every seed is sub-floor → the capability close renders and routes to
    // `find --text` — this is the honest capability statement replacing the retired
    // false repo-absence sentence.
    let result = json!({
        "query": "fsync", "facts": empty_facts(),
        "seeds_available": true,
        "candidates": [candidate_with_score("k0", 0.22)],
    });
    let out = render_find_human(&result, false);
    assert!(
        out.contains("no candidates above the minimum similarity"),
        "seed abstain: {out}"
    );
    // SEED-CHUNK-2 §2.3.3: the capability close keeps only its repo-level "nothing
    // matched" claim (gated on the established fact miss); the `--text` referral moved out
    // to the always-on line below (a copy-paste-runnable command with the actual query).
    assert!(
        out.contains("nothing matched."),
        "capability close states the established fact miss: {out}"
    );
    // review-1 item 3: the query is POSIX-shell-quoted; a shell-safe token like `fsync`
    // renders BARE (no wrapping quotes), so the line runs verbatim.
    assert!(
        out.contains("for exact text, comments, or expressions: rmap find --text fsync\n"),
        "the --text referral routes to a live text scan on the actual query: {out}"
    );
    assert!(
        !out.contains("distinct home"),
        "retired false repo-absence sentence absent: {out}"
    );
}

#[test]
fn malformed_facts_with_subfloor_seeds_withholds_capability_close() {
    // review-1 (§2.4): a MALFORMED facts payload (here a group's `hits` is not a list)
    // is NOT an established miss — "nothing matched" is unproven, so the capability
    // close is WITHHELD (malformed ≠ empty; STANDING HONESTY RULE 1). The seed-tier
    // abstain still renders, and the malformed group is surfaced.
    let mut facts = empty_facts();
    facts[1] = json!({"fact_class": "file", "render_command": "explain", "certainty": "extracted",
        "hits": {"not": "a list"}, "matched": 0, "matched_is_floor": false});
    let result = json!({
        "query": "fsync", "facts": facts,
        "seeds_available": true,
        "candidates": [candidate_with_score("k0", 0.22)],
    });
    let out = render_find_human(&result, false);
    assert!(
        out.contains("no candidates above the minimum similarity"),
        "seed abstain still renders: {out}"
    );
    assert!(
        !out.contains("nothing matched"),
        "no 'nothing matched' when facts are malformed (malformed ≠ empty): {out}"
    );
    assert!(
        out.contains("malformed fact group: hits missing or not a list"),
        "malformed group surfaced: {out}"
    );
}
