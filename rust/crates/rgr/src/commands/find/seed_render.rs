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
pub(super) const SEED_SIMILARITY_FLOOR: f64 = 0.30;

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
    repo_uid: Option<&str>,
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
    // SEED-CHUNK-2 (spec §2.3.3): the `--text` referral renders whenever seeds SERVE
    // (the tier is available and reached here), not only on empty tiers. The query is
    // our OWN DTO field; when absent/empty the referral uses a `<pattern>` placeholder.
    let query = result
        .get("query")
        .and_then(|v| v.as_str())
        .filter(|q| !q.is_empty());
    if candidates.is_empty() {
        out.push_str("  (no area scored above zero)\n");
        append_text_referral(out, query);
        return;
    }

    // FIND-RANK-1 (§2.3): apply the similarity FLOOR. A candidate whose score is present
    // and below the floor is sub-floor hearsay — not rendered, but its score feeds the
    // best-below tracker. A candidate with a present-and-above score OR a MISSING/invalid
    // score is validated by the shared renderer (the latter surfaces as unreadable — the
    // floor never silently drops a malformed candidate; STANDING HONESTY RULE 1).
    //
    // CURSOR-ROUNDTRIP-1 (§2.2): rows render through the ONE shared current-DTO renderer
    // (`presentation::seed::render_seed_chunk_candidate`) that Group B (callers/callees/path
    // not-found fallback) also uses — never a per-command copy (STANDING HONESTY RULE 2). A
    // candidate that fails validation is COUNTED and stated on ONE honest line
    // (`render_unreadable_summary`), never a per-row placeholder (RULE 1).
    // SEED-CHUNK-2 (spec §2.1): bucket the rendered rows into the production and test
    // PARTITIONS (plus an `unknown` bucket for the defensive old-daemon / malformed
    // case, which must never be folded into production). Candidates arrive already ranked
    // production-above-test; we preserve that order within each bucket and, when BOTH the
    // production and test partitions are non-empty, emit ONE partition header between them
    // so the reader sees the moat boundary explicitly — not only the per-row `[test]` label.
    let mut prod = String::new();
    let mut tests = String::new();
    let mut unknown = String::new();
    let mut best_below: Option<f64> = None;
    let mut unreadable: Vec<String> = Vec::new();
    for c in candidates {
        match c.get("score").and_then(|v| v.as_f64()) {
            Some(score) if score < SEED_SIMILARITY_FLOOR => {
                best_below = Some(best_below.map_or(score, |b| b.max(score)));
            }
            _ => match crate::presentation::seed::render_seed_chunk_candidate(c, repo_uid) {
                // Partition by the candidate's OWN `is_test` DTO field. A missing / non-bool
                // value is UNKNOWN classification (old daemon / malformed) — routed to its
                // own bucket, NEVER counted as production; the shared renderer already
                // emitted the `[is_test unknown]` per-row marker (STANDING HONESTY RULE 1).
                Ok(row) => match c.get("is_test") {
                    Some(serde_json::Value::Bool(false)) => prod.push_str(&row),
                    Some(serde_json::Value::Bool(true)) => tests.push_str(&row),
                    _ => unknown.push_str(&row),
                },
                Err(reason) => unreadable.push(reason),
            },
        }
    }
    let rendered_any = !prod.is_empty() || !tests.is_empty() || !unknown.is_empty();
    if rendered_any {
        out.push_str(&prod);
        // The partition header renders ONLY when BOTH partitions are non-empty (spec
        // §2.1): a divider naming the demoted block and restating the moat (production
        // above test). With only one partition present the flat list + per-row labels are
        // already unambiguous, so no header is emitted.
        if !prod.is_empty() && !tests.is_empty() {
            out.push_str("  tests (ranked below production, labeled [test]):\n");
        }
        out.push_str(&tests);
        out.push_str(&unknown);
    }
    if !unreadable.is_empty() {
        out.push_str(&crate::presentation::seed::render_unreadable_summary(
            &unreadable,
        ));
    }
    if !rendered_any {
        if let Some(best) = best_below {
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
            // "nothing matched" there would be false / unproven (review-1; HONESTY RULE 1). The
            // `--text` referral that FIND-GREP-1 tucked into this close moved OUT (SEED-CHUNK-2
            // §2.3.3): it now renders on EVERY serving tier below, so this close keeps only the
            // repo-level "nothing matched" claim it is uniquely entitled to make.
            match fact_outcome {
                super::FactTierOutcome::EstablishedMiss => out.push_str(" — nothing matched."),
                super::FactTierOutcome::MissNotEstablished => {}
            }
            out.push('\n');
        }
    }
    // SEED-CHUNK-2 (spec §2.3.3): the `--text` referral renders whenever seeds SERVE —
    // beside rendered candidates, and on the abstain/known-zero paths — so the reader is
    // always told where exact text/comments/expressions are searched (the `find fsync`
    // capture rendered a seed and never mentioned `--text`; that is the defect this fixes).
    append_text_referral(out, query);
    // (`candidates` is non-empty and every candidate either set `best_below`, rendered a
    // row, or was counted `unreadable` — so at least one arm always fired; no silent empty
    // tier. The per-candidate render + the malformed-count line both live in
    // `presentation::seed`, shared with the Group-B not-found fallback — one renderer.)
}

/// SEED-CHUNK-2 (spec §2.3.3): the one-line `--text` referral appended whenever the seed
/// tier serves. Uses the actual query for a copy-paste-runnable command when present;
/// falls back to a `<pattern>` placeholder when the query is absent/empty (a malformed
/// response — never a fabricated query).
///
/// review-1 item 3: the query is POSIX-shell-quoted via the crate's existing
/// [`super::fact_hit::shell_quote_arg`] so a query containing spaces, `"`, or shell
/// metacharacters yields a RUNNABLE, non-injecting copy-paste line (`--text 'a "b" c'`),
/// not the prior raw double-quote interpolation. The `<pattern>` placeholder (absent
/// query) stays a BARE fill-in marker — it is a human template, not a real argument to
/// run, so it is intentionally left unquoted.
fn append_text_referral(out: &mut String, query: Option<&str>) {
    match query {
        Some(q) => out.push_str(&format!(
            "  for exact text, comments, or expressions: rmap find --text {}\n",
            super::fact_hit::shell_quote_arg(q)
        )),
        None => {
            out.push_str("  for exact text, comments, or expressions: rmap find --text <pattern>\n")
        }
    }
}

#[cfg(test)]
#[path = "seed_render_tests.rs"]
mod tests;
