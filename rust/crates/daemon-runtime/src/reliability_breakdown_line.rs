//! CHECK-LANG-SPLIT-1 (§2): the mixed-repo per-language call-resolution breakdown line — the ONE
//! rendering of `"by language: TypeScript N% of M calls · Java N% of M calls"` that BOTH CI-facing
//! surfaces (`check` and `orient`) route through, so they cannot drift from each other or from
//! `reliability --by-language`.
//!
//! [abstraction: this crate-private module — the mixed-repo per-language reliability breakdown line;
//! concrete current users: `dispatch::handle_check` (→ `CheckInput.reliability_by_language`, rendered
//! under the CALL_GRAPH_RELIABILITY figure) and `orient_coherence` (→
//! `CoherentOrientResult.reliability_by_language`, rendered under the headline reliability caveat) —
//! 2 callers across the two CI-facing surfaces; axis: the two surfaces sharing ONE breakdown so they
//! cannot drift; rejected simpler: compute the line separately in each handler — rejected because that
//! is the exact cross-surface drift this slice exists to prevent. The materiality source stays SINGLE:
//! this module REUSES `crate::reader_context::material_code_languages` (never a re-derived gate) so the
//! breakdown and the D5 CTA share ONE materiality definition.]
//!
//! [abstraction: `orient_reliability_by_language` — orient's breakdown decision over BOTH fallible reads
//! (the materiality file-count read AND the per-language call read); concrete current user:
//! `orient_coherence::compute_briefing_and_remedy` (1 caller); axis: the count-read failure branch needs a
//! test seam that orient_coherence (no test harness; building a `RepoState` is disproportionate) cannot
//! provide, and co-locating orient's two-read orchestration here keeps the failure wording single-sourced;
//! rejected simpler: inline the match in orient — rejected because the count-Err → unknown-with-reason
//! branch (review-2 item 2, STANDING HONESTY RULE 1) would then be untestable without a live daemon. NOT
//! reused by check: check surfaces the SAME count failure in-band via `ceiling_fact = Unknown { reason }`,
//! so routing check through here would double-surface one reason.]

use crate::reader_context::material_code_languages;
use repo_graph_agent::reliability_breakdown::ScopeCountRow;

/// CHECK-LANG-SPLIT-1 (§2): the DISTINCT reader display names of a repo's materially-present code
/// languages (the SAME ≥10%-code-file gate [`material_code_languages`] the CTA uses — REUSED so the
/// breakdown and the CTA share ONE materiality definition), in count-DESC first-appearance order and
/// de-duplicated by display name (so the TS/JS family's multiple indexer tokens — `typescript` + `tsx`,
/// `javascript` + `jsx` — collapse to ONE "TypeScript"/"JavaScript" language exactly as the CTA collapses
/// them). `< 2` distinct names ⇒ a single-language (or unknown-/config-only) repo. `language_counts` MUST
/// arrive count-DESC (as `query_file_count_by_language` returns).
fn distinct_material_displays(language_counts: &[(String, u64)]) -> Vec<&'static str> {
    let mut displays: Vec<&'static str> = Vec::new();
    for m in material_code_languages(language_counts) {
        if !displays.contains(&m.display) {
            displays.push(m.display);
        }
    }
    displays
}

/// CHECK-LANG-SPLIT-1 (§2): is this a MIXED repo — ≥2 materially-present code languages by display name?
/// The cheap gate the check / orient handlers apply on the file-count read they ALREADY hold, BEFORE
/// issuing the (otherwise wasted) per-language call-resolution read, so a single-language repo's output
/// stays byte-identical (slice §2.4: "nothing new") and pays no extra read.
pub(crate) fn is_mixed_material_code_repo(language_counts: &[(String, u64)]) -> bool {
    distinct_material_displays(language_counts).len() >= 2
}

/// CHECK-LANG-SPLIT-1 (§2): the per-language call-resolution breakdown line rendered UNDER the blended
/// reliability figure for a MIXED repo — `"by language: TypeScript 24% of 99 calls · Java 11% of 113
/// calls"`. `None` for a single-language repo (nothing to split — no noise, §2.4).
///
/// One SOURCE, no new metric (§2.1): each cell's figure is the SAME in-scope resolved rate the aggregate
/// / `reliability --by-language` surfaces show, produced by feeding this language's summed counts through
/// the shared [`repo_graph_agent::reliability::language_reliability_cell`] (which routes through the same
/// `ResolvedRate` projection). `by_language` is the EXACT read the `reliability` handler serves
/// (`StorageConnection::query_call_resolution_by_language`) — reachable at check/orient's daemon site
/// without any new public API (precedent: `handlers::reliability`). The ONLY thing added over that read is
/// the grouping: the per-`files.language`-TOKEN reliability rows are summed into one figure PER display
/// name (both the production and test partitions, matching the blended figure which also spans both), so
/// `Σ languages` reconciles to the blended total.
///
/// Display names + the materiality gate come from the SAME [`material_code_languages`] the CTA keys on, so
/// the breakdown and the CTA agree (§2.2). A material language with no CALLS row (present in files, no
/// calls) contributes zero → its cell honestly reads "no in-scope calls measured", never a fabricated 0%.
pub(crate) fn reliability_by_language_line(
    language_counts: &[(String, u64)],
    by_language: &[ScopeCountRow],
) -> Option<String> {
    let displays = distinct_material_displays(language_counts);
    if displays.len() < 2 {
        return None; // single-language / unknown-only — nothing to split.
    }
    let material = material_code_languages(language_counts);
    let cells: Vec<String> = displays
        .iter()
        .map(|display| {
            // Sum this display name's CALLS counts across ALL its material tokens AND both partitions.
            let mut resolved = 0u64;
            let mut internal_like = 0u64;
            for token in material
                .iter()
                .filter(|m| m.display == *display)
                .map(|m| m.token.as_str())
            {
                for row in by_language.iter().filter(|r| r.key == token) {
                    resolved += row.counts.resolved;
                    internal_like += row.counts.internal_like();
                }
            }
            repo_graph_agent::reliability::language_reliability_cell(
                display,
                resolved,
                internal_like,
            )
        })
        .collect();
    Some(format!("by language: {}", cells.join(" · ")))
}

/// CHECK-LANG-SPLIT-1 (§3, STANDING HONESTY RULE 1): the breakdown line, rendering unknown-WITH-REASON
/// when the per-language call-resolution read FAILED — the result is RENDERED, so a failure may never be
/// swallowed to a silent omission. The blended reliability line is independent and stands regardless
/// (spec §3). `None` for a single-language repo REGARDLESS of the read outcome — a repo with nothing to
/// split owes the reader no breakdown and no error (byte-identical, §2.4), so the caller need not even
/// issue the read there. Both CI-facing surfaces route their read result through THIS one wrapper so they
/// render the SAME breakdown on success and the SAME unknown-with-reason on failure.
pub(crate) fn reliability_by_language_line_or_read_error(
    language_counts: &[(String, u64)],
    by_language: Result<Vec<ScopeCountRow>, String>,
) -> Option<String> {
    if !is_mixed_material_code_repo(language_counts) {
        return None; // nothing to split — no breakdown, no error (single-language byte-identical).
    }
    match by_language {
        Ok(rows) => reliability_by_language_line(language_counts, &rows),
        Err(reason) => Some(format!(
            "by language: unavailable — could not read the per-language call breakdown ({reason})"
        )),
    }
}

/// CHECK-LANG-SPLIT-1 (§3, STANDING HONESTY RULE 1 — review-2 item 2): the breakdown line when the
/// MATERIALITY read itself (the per-language FILE counts that decide mixed-ness) FAILED. Mixed-ness is then
/// UNDECIDABLE, so the breakdown surface renders unknown-WITH-REASON rather than a silent `None` a reader
/// would take as "single-language, nothing to split". The blended reliability line is independent and
/// stands (spec §3).
pub(crate) fn language_inventory_unavailable(reason: &str) -> String {
    format!("by language: unavailable — could not read the language inventory ({reason})")
}

/// CHECK-LANG-SPLIT-1 (§2/§3): orient's per-language breakdown decision from the TWO fallible reads it
/// depends on. `language_counts` is the materiality FILE-count read (decides mixed-ness); `read_by_language`
/// is the per-language CALL read, issued LAZILY and ONLY for a decidably-mixed repo — so a single-language
/// repo pays no read and stays byte-identical (§2.4). Every read outcome is honest (STANDING HONESTY RULE 1):
///   - count read FAILED → mixed-ness undecidable → unknown-WITH-REASON (never a silent `None`);
///   - count OK, decidably single-language → `None` (nothing to split, no noise);
///   - count OK + mixed → the breakdown, itself unknown-with-reason if the call read fails.
///
/// The blended line is independent of all of this (§3). Orient's entry point ONLY: check surfaces its count
/// failure in-band via `ceiling_fact = Unknown { reason }`, so check calls `..line_or_read_error` directly
/// on an already-decided count read and does not route through here (routing it here would double-surface).
pub(crate) fn orient_reliability_by_language(
    language_counts: &Result<Vec<(String, u64)>, String>,
    read_by_language: impl FnOnce() -> Result<Vec<ScopeCountRow>, String>,
) -> Option<String> {
    match language_counts {
        Ok(c) if is_mixed_material_code_repo(c) => {
            reliability_by_language_line_or_read_error(c, read_by_language())
        }
        Ok(_) => None,
        Err(reason) => Some(language_inventory_unavailable(reason)),
    }
}

/// CHECK-LANG-SPLIT-1 (review-3): orient's REMEDY + BREAKDOWN decision, pure over injected reads. Extracted
/// so the in-flight/breakdown DECOUPLING — the exact regression review-3 fixed — is unit-testable WITHOUT a
/// live `RepoState` (which orient_coherence cannot build in a unit test). The invariants it encodes are the
/// ones the regression guards:
///   - the breakdown is gated ONLY by `call_graph_has_caveat` (the non-HIGH call-graph axis); it is computed
///     REGARDLESS of `enrich_in_flight`. An in-flight pass is a CAVEAT on the facts (the in-flight posture
///     already tells the reader the figures may still rise), NEVER a reason to hide the split — so orient
///     agrees with `check`, which renders the split in this state too (dispatch.rs, independent of the
///     enrich-state override). The pre-review-3 in-flight early-return dropped the breakdown to `None`, so
///     the two CI-facing surfaces disagreed.
///   - the remedy is the in-flight truth when a pass is in flight (ORIENT-FACT-COHERENCE-1: it SUPERSEDES the
///     per-language CTA), else the LOW-gated CTA, else `None`.
///   - the per-language FILE-count read is issued at most ONCE, and only when the breakdown OR the
///     (non-in-flight) CTA needs it — a genuinely healthy repo, and an in-flight repo whose only count need
///     was the now-suppressed CTA, both short-circuit with no read (the closures stay uncalled).
///
/// [abstraction: `orient_remedy_and_breakdown` — orient's in-flight-aware remedy/breakdown router over three
/// injected fallible reads; concrete current user: `orient_coherence::compute_briefing_and_remedy` (1
/// caller); axis: a TEST SEAM the reviewer (review-3 item 2) required for the in-flight decoupling that
/// orient_coherence (no `RepoState` test harness; and the in-flight MEDIUM-mixed state is unreachable in the
/// isolated dogfood — SCIP enrichment unwired) cannot cover any other way; rejected simpler: inline the
/// routing in orient with no unit test — rejected because the review-3 fix would then have ZERO regression
/// coverage. Not reused by check: check has no in-flight/CTA suppression on its breakdown path and surfaces
/// its count failure in-band via `ceiling_fact = Unknown`.]
#[allow(clippy::too_many_arguments)]
pub(crate) fn orient_remedy_and_breakdown(
    call_graph_has_caveat: bool,
    remedy_is_low: bool,
    enrich_in_flight: bool,
    in_flight_wording: impl FnOnce() -> String,
    read_counts: impl FnOnce() -> Result<Vec<(String, u64)>, String>,
    read_by_language: impl FnOnce() -> Result<Vec<ScopeCountRow>, String>,
    cta_line: impl FnOnce(Result<Vec<(String, u64)>, String>) -> Option<String>,
) -> (Option<String>, Option<String>) {
    // ORIENT-FACT-COHERENCE-1: while a pass is in flight the honest remedy is the in-flight truth; it
    // supersedes the per-language CTA. Computed lazily (no wording minted when not in flight).
    let in_flight_remedy = enrich_in_flight.then(in_flight_wording);
    // The CTA needs the counts only when the repo is LOW *and* the in-flight remedy is NOT superseding it.
    let cta_needs_counts = remedy_is_low && !enrich_in_flight;
    let (cta_remedy, breakdown) = if call_graph_has_caveat || cta_needs_counts {
        // ONE count read, shared by the breakdown and the CTA. Borrowed by the breakdown BEFORE the CTA
        // consumes it.
        let language_counts = read_counts();
        let breakdown = if call_graph_has_caveat {
            orient_reliability_by_language(&language_counts, read_by_language)
        } else {
            // An import-graph-only LOW (call-graph HIGH, relationship LOW) has no call figure to decompose.
            None
        };
        let cta = if cta_needs_counts {
            cta_line(language_counts)
        } else {
            None
        };
        (cta, breakdown)
    } else {
        (None, None)
    };
    (in_flight_remedy.or(cta_remedy), breakdown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_agent::reliability_breakdown::CallResolutionCounts;

    /// Repo file-count pairs (count-DESC), as `query_file_count_by_language` returns.
    fn counts(pairs: &[(&str, u64)]) -> Vec<(String, u64)> {
        pairs.iter().map(|(l, n)| (l.to_string(), *n)).collect()
    }

    /// One per-`(language token, is_test)` call-resolution row, as `query_call_resolution_by_language`
    /// returns. `external`/`unknown` default to 0 (in-scope calls) unless a case needs them.
    fn lrow(key: &str, resolved: u64, unresolved: u64) -> ScopeCountRow {
        ScopeCountRow {
            key: key.to_string(),
            is_test: false,
            counts: CallResolutionCounts {
                resolved,
                unresolved,
                external: 0,
                unknown: 0,
            },
        }
    }

    #[test]
    fn breakdown_two_language_mix_names_each_display_with_its_rate() {
        // glamCRM shape: TS-dominant + Java. TS 24/(24+76)=24% of 100; Java 12/(12+101)=11% of 113.
        let language_counts = counts(&[("typescript", 600), ("java", 300)]);
        let by_language = vec![lrow("typescript", 24, 76), lrow("java", 12, 101)];
        let line = reliability_by_language_line(&language_counts, &by_language)
            .expect("a 2-language mix renders a breakdown");
        assert_eq!(
            line, "by language: TypeScript 24% of 100 calls · Java 11% of 113 calls",
            "{line}"
        );
        // Order is file-count-DESC (dominant first), matching the CTA's material order.
        assert!(line.find("TypeScript").unwrap() < line.find("Java").unwrap());
    }

    #[test]
    fn breakdown_three_language_mix_lists_all_three() {
        let language_counts = counts(&[("typescript", 500), ("java", 300), ("python", 200)]);
        let by_language = vec![
            lrow("typescript", 30, 70),
            lrow("java", 12, 101),
            lrow("python", 5, 95),
        ];
        let line = reliability_by_language_line(&language_counts, &by_language).unwrap();
        assert_eq!(
            line,
            "by language: TypeScript 30% of 100 calls · Java 11% of 113 calls · Python 5% of 100 calls",
            "{line}"
        );
    }

    #[test]
    fn breakdown_single_language_repo_renders_nothing() {
        // A pure-TS repo (json has no display name → not a code language) is single-language → no split.
        assert!(
            reliability_by_language_line(&counts(&[("typescript", 900), ("json", 100)]), &[])
                .is_none()
        );
        // Empty / unknown-only likewise.
        assert!(reliability_by_language_line(&[], &[]).is_none());
        assert!(reliability_by_language_line(&counts(&[("other", 500)]), &[]).is_none());
    }

    #[test]
    fn breakdown_groups_ts_js_family_tokens_under_one_display_name() {
        // .ts + .tsx are BOTH the TypeScript family (distinct indexer tokens): they must collapse to ONE
        // "TypeScript" cell (summed), so the breakdown's display names AGREE with the CTA's (which also
        // dedups the family to one name). typescript 10/40 + tsx 14/60 → 24 resolved / 100 internal_like.
        let language_counts = counts(&[("typescript", 400), ("tsx", 300), ("java", 300)]);
        let by_language = vec![
            lrow("typescript", 10, 40),
            lrow("tsx", 14, 60),
            lrow("java", 12, 101),
        ];
        let line = reliability_by_language_line(&language_counts, &by_language).unwrap();
        assert_eq!(
            line, "by language: TypeScript 19% of 124 calls · Java 11% of 113 calls",
            "{line}"
        );
        // Exactly ONE "TypeScript" cell (no duplicate family token) — the CTA-agreement invariant.
        assert_eq!(line.matches("TypeScript").count(), 1, "{line}");
    }

    #[test]
    fn breakdown_material_language_with_no_calls_reads_unknown_never_a_fabricated_percent() {
        // Go is materially present in FILES but has no CALLS row → unknown, never 0%/100% (VISION).
        let language_counts = counts(&[("typescript", 500), ("go", 400)]);
        let by_language = vec![lrow("typescript", 30, 70)]; // no `go` row
        let line = reliability_by_language_line(&language_counts, &by_language).unwrap();
        assert!(line.contains("Go no in-scope calls measured"), "{line}");
        assert!(
            !line.contains("Go 0%") && !line.contains("Go 100%"),
            "{line}"
        );
    }

    #[test]
    fn breakdown_read_error_renders_unknown_with_reason_only_for_a_mixed_repo() {
        // A LOW mixed repo owes the reader the breakdown; a failed per-language read renders
        // unknown-WITH-REASON, carrying the error — never a silent omission (STANDING HONESTY RULE 1).
        let mixed = counts(&[("typescript", 600), ("java", 300)]);
        let line = reliability_by_language_line_or_read_error(&mixed, Err("db locked".to_string()))
            .expect("a mixed repo surfaces the read failure");
        assert!(line.contains("unavailable"), "{line}");
        assert!(
            line.contains("db locked"),
            "the reason is preserved: {line}"
        );
        // A single-language repo owes NOTHING — no breakdown and no error even on a failed read (§2.4).
        assert!(reliability_by_language_line_or_read_error(
            &counts(&[("typescript", 900)]),
            Err("db locked".to_string())
        )
        .is_none());
    }

    #[test]
    fn language_inventory_unavailable_renders_unknown_with_reason() {
        let line = language_inventory_unavailable("db locked");
        assert!(line.contains("unavailable"), "{line}");
        assert!(
            line.contains("db locked"),
            "the reason is preserved: {line}"
        );
    }

    #[test]
    fn orient_count_read_failure_renders_unknown_with_reason_never_silent() {
        // review-2 item 2 (STANDING HONESTY RULE 1): orient's materiality FILE-count read failed →
        // mixed-ness is undecidable → unknown-WITH-REASON, never a silent `None` a reader would take as
        // "single-language, nothing to split". The per-language read is NOT even attempted (the panic
        // proves it stays lazy on the count-Err branch).
        let counts_err: Result<Vec<(String, u64)>, String> = Err("db locked".to_string());
        let line = orient_reliability_by_language(&counts_err, || {
            panic!("per-language read must NOT run when the count read failed")
        })
        .expect("a failed count read surfaces unknown-with-reason");
        assert!(line.contains("unavailable"), "{line}");
        assert!(
            line.contains("db locked"),
            "the reason is preserved: {line}"
        );
    }

    #[test]
    fn orient_single_language_repo_reads_nothing_and_renders_nothing() {
        // Decidably single-language (count OK, one material display) → `None`, and the per-language read is
        // never issued (byte-identical, §2.4; the panic proves the read is skipped).
        let counts_ok: Result<Vec<(String, u64)>, String> = Ok(counts(&[("typescript", 900)]));
        assert!(orient_reliability_by_language(&counts_ok, || panic!(
            "single-language repo must issue no per-language read"
        ))
        .is_none());
    }

    #[test]
    fn orient_mixed_repo_renders_the_breakdown_from_the_lazy_read() {
        // Count OK + mixed → the per-language read IS issued and the breakdown renders (same figures as
        // `reliability_by_language_line`), proving orient's happy path routes through the shared line.
        let counts_ok: Result<Vec<(String, u64)>, String> =
            Ok(counts(&[("typescript", 600), ("java", 300)]));
        let line = orient_reliability_by_language(&counts_ok, || {
            Ok(vec![lrow("typescript", 24, 76), lrow("java", 12, 101)])
        })
        .expect("a mixed repo renders the breakdown");
        assert_eq!(
            line, "by language: TypeScript 24% of 100 calls · Java 11% of 113 calls",
            "{line}"
        );
    }

    #[test]
    fn orient_mixed_repo_with_failed_call_read_renders_unknown_with_reason() {
        // Count OK + mixed, but the per-language CALL read failed → unknown-WITH-REASON (routes through
        // `reliability_by_language_line_or_read_error`), never a silent omission.
        let counts_ok: Result<Vec<(String, u64)>, String> =
            Ok(counts(&[("typescript", 600), ("java", 300)]));
        let line = orient_reliability_by_language(&counts_ok, || Err("db locked".to_string()))
            .expect("a mixed repo surfaces the call-read failure");
        assert!(line.contains("unavailable"), "{line}");
        assert!(
            line.contains("db locked"),
            "the reason is preserved: {line}"
        );
    }

    #[test]
    fn orient_in_flight_mixed_caveat_repo_retains_the_breakdown_and_shows_in_flight_remedy() {
        // review-3 REGRESSION: a mixed, non-HIGH call-graph repo with enrichment IN FLIGHT must retain the
        // shared breakdown — matching `check`, which renders the split in this state. This is the strongest
        // case: MEDIUM (`remedy_is_low = false`), so pre-fix the in-flight early-return produced
        // `(briefing, Some(in_flight), None)` and the ONLY thing lost was the breakdown, leaving the two
        // CI-facing surfaces disagreeing. The CTA closure PANICS to prove it is never consulted while a pass
        // is in flight (the in-flight remedy supersedes it).
        let (remedy, breakdown) = orient_remedy_and_breakdown(
            /* call_graph_has_caveat */ true,
            /* remedy_is_low */ false,
            /* enrich_in_flight */ true,
            || "ENRICHMENT_STATE: in flight".to_string(),
            || Ok(counts(&[("typescript", 600), ("java", 300)])),
            || Ok(vec![lrow("typescript", 24, 76), lrow("java", 12, 101)]),
            |_counts| panic!("the per-language CTA must NOT run while a pass is in flight"),
        );
        assert_eq!(
            remedy.as_deref(),
            Some("ENRICHMENT_STATE: in flight"),
            "the in-flight truth is the remedy"
        );
        assert_eq!(
            breakdown.as_deref(),
            Some("by language: TypeScript 24% of 100 calls · Java 11% of 113 calls"),
            "the breakdown is RETAINED in flight, identical to the non-in-flight render (review-3 fix)"
        );
    }

    #[test]
    fn orient_in_flight_low_repo_suppresses_the_cta_but_keeps_the_breakdown() {
        // A LOW mixed repo in flight: the per-language CTA is SUPPRESSED (superseded by the in-flight truth),
        // yet the breakdown still renders. The CTA closure panics to prove suppression is by short-circuit,
        // not by discarding a computed value.
        let (remedy, breakdown) = orient_remedy_and_breakdown(
            true,
            true, // low
            true, // in flight
            || "ENRICHMENT_STATE: in flight".to_string(),
            || Ok(counts(&[("typescript", 600), ("java", 300)])),
            || Ok(vec![lrow("typescript", 24, 76), lrow("java", 12, 101)]),
            |_counts| panic!("the per-language CTA must NOT run while a pass is in flight"),
        );
        assert_eq!(remedy.as_deref(), Some("ENRICHMENT_STATE: in flight"));
        assert_eq!(
            breakdown.as_deref(),
            Some("by language: TypeScript 24% of 100 calls · Java 11% of 113 calls")
        );
    }

    #[test]
    fn orient_in_flight_no_caveat_issues_no_read_and_shows_only_the_in_flight_remedy() {
        // In flight, but the call-graph axis is HIGH (no caveat) and no non-in-flight CTA is needed → NO
        // reads are issued (the panics prove the healthy short-circuit survives the in-flight branch) and the
        // breakdown is `None` (nothing to split).
        let (remedy, breakdown) = orient_remedy_and_breakdown(
            /* call_graph_has_caveat */ false,
            /* remedy_is_low */ false,
            /* enrich_in_flight */ true,
            || "ENRICHMENT_STATE: in flight".to_string(),
            || panic!("no count read without a caveat or a non-in-flight CTA"),
            || panic!("no per-language read without a breakdown surface"),
            |_counts| panic!("no CTA read while in flight"),
        );
        assert_eq!(remedy.as_deref(), Some("ENRICHMENT_STATE: in flight"));
        assert!(breakdown.is_none());
    }

    #[test]
    fn orient_not_in_flight_low_mixed_renders_both_cta_and_breakdown_from_one_read() {
        // The non-in-flight path is UNCHANGED (pre-review-3 behavior verbatim): a LOW mixed repo renders the
        // per-language CTA AND the breakdown, both from the ONE shared count read. The in-flight-wording
        // closure panics to prove it is never minted when not in flight.
        let (remedy, breakdown) = orient_remedy_and_breakdown(
            true,
            true,
            false,
            || panic!("no in-flight wording when not in flight"),
            || Ok(counts(&[("typescript", 600), ("java", 300)])),
            || Ok(vec![lrow("typescript", 24, 76), lrow("java", 12, 101)]),
            |language_counts| language_counts.as_ref().ok().map(|_| "CTA".to_string()),
        );
        assert_eq!(remedy.as_deref(), Some("CTA"));
        assert_eq!(
            breakdown.as_deref(),
            Some("by language: TypeScript 24% of 100 calls · Java 11% of 113 calls")
        );
    }

    #[test]
    fn orient_healthy_repo_issues_no_read_and_renders_nothing() {
        // No caveat, not low, not in flight → the whole surface short-circuits with NO reads (healthy hot
        // path preserved — every closure panics if touched).
        let (remedy, breakdown) = orient_remedy_and_breakdown(
            false,
            false,
            false,
            || panic!("not in flight"),
            || panic!("healthy repo issues no count read"),
            || panic!("healthy repo issues no per-language read"),
            |_language_counts| panic!("healthy repo renders no CTA"),
        );
        assert!(remedy.is_none());
        assert!(breakdown.is_none());
    }

    #[test]
    fn orient_not_in_flight_low_with_failed_count_read_still_renders_the_cta_wrapper() {
        // Non-in-flight, LOW, failed count read → the breakdown surfaces unknown-with-reason AND the CTA
        // wrapper still runs on the same failed read (its own honest handling lives in the CTA wrapper).
        // Proves the shared single read reaches both consumers even on the Err path.
        let (remedy, breakdown) = orient_remedy_and_breakdown(
            true,
            true,
            false,
            || panic!("not in flight"),
            || Err("db locked".to_string()),
            || panic!("per-language read must NOT run when the count read failed"),
            |language_counts| language_counts.err().map(|e| format!("cta-saw: {e}")),
        );
        assert_eq!(remedy.as_deref(), Some("cta-saw: db locked"));
        let breakdown =
            breakdown.expect("failed count read surfaces unknown-with-reason breakdown");
        assert!(breakdown.contains("unavailable"), "{breakdown}");
        assert!(breakdown.contains("db locked"), "{breakdown}");
    }
}
