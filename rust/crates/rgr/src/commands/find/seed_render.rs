//! The `find` DEMOTED semantic-seed tier renderer (FIND-FACTS-1 §2.3): the renamed
//! ranked-guesses header, then either the candidates or an explicit
//! `semantic seeds unavailable (<reason>)`. Every identity field is our OWN DTO; a
//! genuinely-absent required field is MALFORMED and surfaced, NEVER fabricated
//! (STANDING HONESTY RULE 1). Split off `find.rs` per the ≤500-line guardrail
//! (review-4 item 1).
//!
//! Abstraction record — module: `find::seed_render`; concrete current user:
//! `find::render_find_human`; axis: the ≤500-line guardrail; rejected simpler
//! alternative: inlining in `find.rs` (file stays >500).

/// FIND-RANK-1 (§2.3) — the seed SIMILARITY FLOOR. A pinned constant (NOT adaptive):
/// seeds scoring below it are hearsay padding an empty answer and are not rendered;
/// when ALL seeds fall below it the tier ABSTAINS with the honest best-score line.
///
/// # Basis — potion-code-16M-v2 calibration (SEED-CHUNK-1, operator ruling Option C)
///
/// The old 0.60 was NOMIC/LM-Studio geometry (a no-home band 0.50–0.54); carrying it
/// to potion's different, corpus-relative geometry was itself an unfounded certainty
/// claim (review-1). Re-measured in the isolated rig against the four corpora
/// (build report `.agent-manager/slices/SEED-CHUNK-1/build-3.md`; raw table
/// `/private/tmp/seedchunk-cal/calibration-table.md`):
///
/// - potion's no-home band spans **0.18–0.59**. The high tail is single-word LEXICAL
///   collision (repo-graph `find "MIDI audio synthesis"` → `FileArtifact.synthesisMode`
///   0.589; glamCRM `find "genome sequence alignment"` → `Etapa.sequence` 0.494).
/// - the spike's true hits: leveldb `DBImpl::Recover` **0.322**, `RemoveObsoleteFiles`
///   **0.360**; repo-graph 0.69–0.78.
/// - **These bands OVERLAP.** Within leveldb ALONE, a genuine no-home concept
///   (`"react component hooks state"` → 0.338) outscores the real ground-truth
///   `DBImpl::Recover` (0.322). So NO fixed global floor both renders leveldb's true
///   hits (needs ≤0.32) and abstains on the no-home band (needs >0.59).
///
/// This value **0.30** is RATIFIED (operator ruling SEEDCHUNK-FLOOR-2, 2026-09-04) with
/// its MEANING demoted to match the measurement: it is a NOISE-TAIL cutoff, NOT a
/// certainty threshold. It reproduces BOTH leveldb spike ground truths through the
/// product (0.322 / 0.360 clear it) and abstains only on the lowest no-home tail
/// (0.18–0.29). It does NOT — cannot — abstain on lexical-collision no-home hits
/// (0.46–0.59); for potion the fixed floor is a WEAK quality gate, not a clean
/// signal/noise separator (the no-home band OVERLAPS the true-hit band above). The
/// Layer-3 honesty is carried by the "ranked guesses, not facts" framing, the facts
/// wall, and the production/test partition — NOT by this floor; the floor only trims the
/// hopeless tail. Option C's "floor above the no-home band" was proven unsatisfiable
/// alongside the DoD (the falsified premise), so the ruling fixed 0.30 and demoted its
/// meaning rather than raising it. The rendered abstain line therefore says "no
/// candidates above the minimum similarity 0.30" — never wording implying calibrated
/// certainty (ruling condition 2).
///
/// PRESENTATION-side filtering only: pins + ranking formula are frozen (§3); JSON still
/// carries every candidate with its raw `score` for a programmatic consumer to filter.
const SEED_SIMILARITY_FLOOR: f64 = 0.30;

/// Render the DEMOTED semantic-seed tier (§2.3): the renamed header, then either the
/// ranked guesses or an explicit `semantic seeds unavailable (<reason>)`.
///
/// `fact_outcome` is the FACTS tier's own report (`render_facts_tier`) of whether it
/// established a miss. It gates ONLY the §2.4 capability close appended to the sub-floor
/// abstain: that close claims "nothing matched" about the whole repo, true ONLY on an
/// established fact-table miss (review-1). The seed-tier abstain itself (no seed cleared
/// the floor) renders regardless — that is honest about the seed tier alone.
pub(super) fn render_seed_tier(
    result: &serde_json::Value,
    fact_outcome: super::FactTierOutcome,
    out: &mut String,
) {
    out.push_str("Semantic seeds (embedding similarity — ranked guesses, not facts):\n");

    // `seeds_available` is our OWN DTO field, ALWAYS serialized (bool). A missing /
    // mistyped value is MALFORMED — surfaced, NEVER defaulted to `false` (which would
    // render a fabricated "unavailable" over a possibly-available tier; STANDING
    // HONESTY RULE 1). Absent ≠ false.
    let available = match result.get("seeds_available").and_then(|v| v.as_bool()) {
        Some(a) => a,
        None => {
            out.push_str("  (malformed find response: seeds_available missing or not a bool)\n");
            return;
        }
    };
    if !available {
        // When the tier is unavailable, the DTO ALWAYS carries the reason
        // (`build_find_response`: every `available == false` arm sets
        // `seeds_unavailable_reason`). Its absence here is MALFORMED — surfaced, never
        // a fabricated generic "reason unavailable" (STANDING HONESTY RULE 1).
        match result
            .get("seeds_unavailable_reason")
            .and_then(|v| v.as_str())
        {
            Some(reason) => out.push_str(&format!("  semantic seeds unavailable ({reason})\n")),
            None => {
                out.push_str("  (malformed find response: seeds unavailable but no reason given)\n")
            }
        }
        return;
    }

    // `candidates` is our OWN DTO field, ALWAYS serialized (`[]` when empty). A
    // missing key / non-array is MALFORMED, not a genuine zero.
    let candidates = match result.get("candidates") {
        Some(serde_json::Value::Array(a)) => a,
        _ => {
            out.push_str("  (malformed find response: candidates missing or not a list)\n");
            return;
        }
    };
    if candidates.is_empty() {
        out.push_str("  (no area scored above zero)\n");
        return;
    }

    // FIND-RANK-1 (§2.3): apply the similarity FLOOR. A candidate whose score is present
    // and below the floor is sub-floor hearsay — not rendered, but its score feeds the
    // best-below tracker. A candidate with a present-and-above score OR a MISSING/invalid
    // score is rendered (the latter surfaces as malformed inside `render_seed_candidate`
    // — the floor never silently drops a malformed candidate; STANDING HONESTY RULE 1).
    let mut rendered = String::new();
    let mut rendered_any = false;
    let mut best_below: Option<f64> = None;
    for c in candidates {
        match c.get("score").and_then(|v| v.as_f64()) {
            Some(score) if score < SEED_SIMILARITY_FLOOR => {
                best_below = Some(best_below.map_or(score, |b| b.max(score)));
            }
            _ => {
                rendered.push_str(&render_seed_candidate(c));
                rendered_any = true;
            }
        }
    }
    if rendered_any {
        out.push_str(&rendered);
    } else if let Some(best) = best_below {
        // ALL seeds fell below the floor → the honest ABSTAIN (§2.3), never 10 rows of
        // sub-floor hearsay padding an empty answer. `best` is the highest sub-floor
        // score, stated so the reader sees exactly how far below the floor the corpus is.
        //
        // Wording (operator ruling SEEDCHUNK-FLOOR-2 condition 2): the floor is a
        // NOISE-TAIL cutoff, NOT a calibrated signal/noise separator (the calibration
        // proved the no-home band overlaps the true-hit band for potion). So this line
        // says "no candidates above the minimum similarity <floor>" — it must NOT imply
        // the floor separates signal from noise. The Layer-3 honesty is carried by the
        // "ranked guesses, not facts" header (above) + the facts wall + the
        // production/test partition; the floor only trims the hopeless tail.
        out.push_str(&format!(
            "  no candidates above the minimum similarity {SEED_SIMILARITY_FLOOR:.2} (best: {best:.2})"
        ));
        // FIND-GREP-1 (§2.4 / §4): the CAPABILITY close. The old close — "the concept may
        // not have a distinct home in this repo" — was measured FALSE 3/3 on literal probes
        // (fsync/TODO/unwrap_or all PRESENT): the gap is a CAPABILITY one (facts index
        // symbols, not text), never a repo-absence one. §2.4 scopes the replacement close
        // to a fact-table MISS: it claims "nothing matched" about the repo, true ONLY when
        // the facts tier honestly established a miss. When facts MATCHED (or the payload was
        // malformed/incomplete, so no miss is established) the close is WITHHELD — appending
        // "nothing matched" there would be false / unproven (review-1; HONESTY RULE 1).
        match fact_outcome {
            super::FactTierOutcome::EstablishedMiss => out.push_str(
                " — nothing matched; for literal text, comments, or expressions try `rmap find --text \"<pattern>\"`.",
            ),
            super::FactTierOutcome::MissNotEstablished => {}
        }
        out.push('\n');
    }
    // (`candidates` is non-empty and every candidate either set `best_below` or rendered,
    // so one of the two arms always fired — no silent empty tier.)
}

/// Render one seed candidate (unchanged validation from the prior slice): every
/// identity field is our own DTO; a genuinely-absent required field is MALFORMED and
/// surfaced, NEVER fabricated. The provenance label is the daemon's own VALIDATED
/// `source` (must be `embedding`), never a literal pasted over the payload.
fn render_seed_candidate(c: &serde_json::Value) -> String {
    let path = c.get("path").and_then(|v| v.as_str());
    let key = c.get("stable_key").and_then(|v| v.as_str());
    let score = c.get("score").and_then(|v| v.as_f64());
    let model = c.get("model_id").and_then(|v| v.as_str());
    let source = c.get("source").and_then(|v| v.as_str());
    let (Some(path), Some(key), Some(score), Some(model), Some(source)) =
        (path, key, score, model, source)
    else {
        return
            "  (malformed candidate: missing required field — path/stable_key/score/model_id/source)\n"
                .to_string();
    };
    if source != "embedding" {
        return format!(
            "  (malformed candidate: source {source:?} is not a Layer-3 embedding hint)\n"
        );
    }
    let Some(module) = render_module_hint(c.get("module")) else {
        return
            "  (malformed candidate: missing or invalid module hint — expected owning/unavailable)\n"
                .to_string();
    };
    // SEED-CHUNK-1: seeds are per-SYMBOL chunks, so the anchor is `path:line` +
    // qualified name (FIND-EVIDENCE-1 discipline). `line`/`qualified_name` are additive
    // (skip-serialized when the node had no stored span/name) — absent renders WITHOUT
    // them, never a fabricated 0. `is_test` (always serialized) labels the DEMOTED test
    // block so a test hit is never mistaken for production (spec §5).
    let anchor = match c.get("line").and_then(|v| v.as_i64()) {
        Some(line) => format!("{path}:{line}"),
        None => path.to_string(),
    };
    let symbol = c
        .get("qualified_name")
        .and_then(|v| v.as_str())
        .map(|q| format!("  {q}"))
        .unwrap_or_default();
    // `is_test` is our OWN DTO field, ALWAYS serialized (spec §5 the moat). Production
    // (`false`) is the unlabeled default (it ranks above the wall); a test chunk is
    // labeled `[test]`. A MISSING / non-bool `is_test` is UNKNOWN classification —
    // rendered with an explicit marker, NEVER left blank to masquerade as production
    // (review-1 gap d; STANDING HONESTY: unknown is never invisible).
    let test_label = match c.get("is_test") {
        Some(serde_json::Value::Bool(true)) => "  [test]",
        Some(serde_json::Value::Bool(false)) => "",
        _ => "  [is_test unknown]",
    };
    let mut s = format!(
        "  {anchor}{symbol}  (score {score:.2}, {source}, model {model}{module}){test_label}\n"
    );
    s.push_str(&render_next(c.get("next"), key));
    s
}

/// Render a candidate's `next` follow-up line. `cwd` is OPTIONAL (operator ruling 2):
/// when the registry resolved the repo root it prints the `cd <cwd> && …` hint; when
/// the lookup was unavailable it prints the honest reason from `next.cwd_unavailable`
/// (never a fabricated empty cwd). A `next` object carrying NEITHER is malformed.
fn render_next(next: Option<&serde_json::Value>, key: &str) -> String {
    let Some(n) = next.and_then(|v| v.as_object()) else {
        return "    (malformed candidate: missing next follow-up)\n".to_string();
    };
    let cwd = n.get("cwd").and_then(|v| v.as_str());
    let unavailable = n.get("cwd_unavailable").and_then(|v| v.as_str());
    match (cwd, unavailable) {
        (Some(cwd), _) => format!("    → (cd {cwd} && rmap explain {key})\n"),
        (None, Some(reason)) => {
            format!(
                "    → rmap explain {key}  (run from the repo root — working directory {reason})\n"
            )
        }
        (None, None) => {
            "    (malformed candidate: next has neither cwd nor a reason)\n".to_string()
        }
    }
}

/// Render the owning-module hint (`ModuleHint`, externally tagged) for human mode:
/// `Some(", module <path>")` when a genuine module is known, `Some(", module: <reason>")`
/// when explicitly unavailable. Returns `None` when the field is absent or is neither
/// tagged shape — a MALFORMED candidate (our own DTO always carries one of the two), which
/// the caller surfaces rather than rendering as "no module required". Never fabricates.
fn render_module_hint(module: Option<&serde_json::Value>) -> Option<String> {
    let m = module?.as_object()?;
    if let Some(path) = m.get("owning").and_then(|v| v.as_str()) {
        return Some(format!(", module {path}"));
    }
    if let Some(reason) = m.get("unavailable").and_then(|v| v.as_str()) {
        return Some(format!(", module: {reason}"));
    }
    None
}

#[cfg(test)]
mod tests {
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
            &mut out,
        );
        assert!(
            out.contains(
                "no candidates above the minimum similarity 0.30 (best: 0.14) — nothing matched; for literal text, comments, or expressions try `rmap find --text \"<pattern>\"`."
            ),
            "abstain states capability, not repo absence, in noise-tail-cutoff wording: {out}"
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
    }

    #[test]
    fn seed_below_floor_with_missing_score_is_still_surfaced_not_swallowed() {
        // A candidate with NO score cannot be floor-filtered — it must still render (and
        // surface as malformed), never silently dropped by the floor (HONESTY RULE 1).
        let mut out = String::new();
        render_seed_tier(
            &json!({"seeds_available": true, "candidates": [{"path": "src/x.ts"}]}),
            FactTierOutcome::EstablishedMiss,
            &mut out,
        );
        assert!(
            out.contains("malformed candidate"),
            "malformed surfaced: {out}"
        );
    }

    #[test]
    fn well_formed_candidate_renders_with_validated_label() {
        let mut out = String::new();
        let result = json!({
            "seeds_available": true,
            "candidates": [well_formed_candidate(json!("embedding"))],
        });
        render_seed_tier(&result, FactTierOutcome::EstablishedMiss, &mut out);
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
            &mut out,
        );
        assert!(
            out.contains("db/db_impl.cc:415  leveldb::DBImpl::RecoverLogFile  (score 0.71"),
            "production chunk anchor + qualified name: {out}"
        );
        assert!(
            out.contains(
                "db/recovery_test.cc:30  leveldb::RecoveryTest::OpenWithStatus  (score 0.65"
            ) && out.contains("[test]"),
            "test chunk is anchored AND labeled [test]: {out}"
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
    fn non_embedding_seed_source_is_malformed_never_relabeled() {
        let _ = empty_facts(); // fixture parity with the other tiers' tests.
        let mut out = String::new();
        let result = json!({
            "seeds_available": true,
            "candidates": [well_formed_candidate(json!("lexical"))],
        });
        render_seed_tier(&result, FactTierOutcome::EstablishedMiss, &mut out);
        assert!(
            out.contains("malformed candidate"),
            "surfaced malformed: {out}"
        );
        assert!(out.contains("\"lexical\""), "names offending source: {out}");
        assert!(!out.contains("0.71, embedding"), "not relabeled: {out}");
    }

    #[test]
    fn seeds_available_missing_is_malformed_never_defaulted() {
        let mut out = String::new();
        render_seed_tier(
            &json!({"candidates": []}),
            FactTierOutcome::EstablishedMiss,
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
            &mut out,
        );
        assert!(
            out.contains("malformed find response: seeds unavailable but no reason given"),
            "{out}"
        );
    }
}
