//! Phase 1: Condition evaluation.
//!
//! A pure function that takes `CheckInput` and produces a
//! `Vec<ConditionResult>`, one per condition code. When no
//! snapshot exists, only `SNAPSHOT_EXISTS` is evaluated (with
//! status Incomplete); conditions 2-7 are omitted entirely.

use crate::reliability::{self, sentence_case, CallReliabilityView};
use crate::storage_port::{AgentReliabilityLevel, EnrichmentState};

use super::types::{
    CheckInput, ConditionCode, ConditionResult, ConditionStatus, GateOutcomeForCheck,
};

// ── Shared ENRICHMENT_STATE reader vocabulary (RESOLUTION-BREAKDOWN-CLI-1 F1) ──
//
// The four reader-facing summaries for the enrichment axis, extracted from the
// inline literals below so the `reliability` breakdown surface can render the SAME
// wording (review-0 F1: "include the shared enrichment state without inventing new
// wording"). ONE home; check maps its Pass/Fail/Incomplete status separately. Byte
// identity on `check` is the guard that the extraction changed nothing.

/// `EnrichmentState::Ran` — the enrichment phase executed (yield may be zero).
pub const ENRICHMENT_SUMMARY_RAN: &str = "Enrichment phase executed.";
/// `EnrichmentState::NotApplicable` — no eligible edges to enrich.
pub const ENRICHMENT_SUMMARY_NOT_APPLICABLE: &str = "No eligible edges for enrichment.";
/// `EnrichmentState::NotRun` — eligible edges existed but the phase never ran.
pub const ENRICHMENT_SUMMARY_NOT_RUN: &str = "Enrichment phase did not run.";
/// No enrichment state available (e.g. the trust summary could not be assembled).
pub const ENRICHMENT_SUMMARY_UNAVAILABLE: &str = "Enrichment state data unavailable.";

/// The reader-frame ENRICHMENT_STATE summary for a (possibly absent) state — the
/// SINGLE source of this wording, called by both `check`'s condition and the
/// `reliability` breakdown's enrichment line. `None` = state unavailable.
pub fn enrichment_state_summary(state: Option<EnrichmentState>) -> &'static str {
    match state {
        Some(EnrichmentState::Ran) => ENRICHMENT_SUMMARY_RAN,
        Some(EnrichmentState::NotApplicable) => ENRICHMENT_SUMMARY_NOT_APPLICABLE,
        Some(EnrichmentState::NotRun) => ENRICHMENT_SUMMARY_NOT_RUN,
        None => ENRICHMENT_SUMMARY_UNAVAILABLE,
    }
}

/// The machine wire token for the enrichment state — the SAME snake_case tokens the
/// `repo_graph_enrichment` crate serializes (`ran`/`not_run`/`not_applicable`), so the
/// `reliability` JSON surface speaks the established vocabulary. `None` = unavailable
/// (serialized as JSON `null`, never a fabricated token).
pub fn enrichment_state_token(state: Option<EnrichmentState>) -> Option<&'static str> {
    match state {
        Some(EnrichmentState::Ran) => Some("ran"),
        Some(EnrichmentState::NotApplicable) => Some("not_applicable"),
        Some(EnrichmentState::NotRun) => Some("not_run"),
        None => None,
    }
}

/// Evaluate all applicable conditions from the pre-fetched input.
///
/// Returns one `ConditionResult` per evaluated condition code.
/// When `input.snapshot_exists` is false, only the
/// `SNAPSHOT_EXISTS` condition is returned.
pub fn evaluate_conditions(input: &CheckInput) -> Vec<ConditionResult> {
    let mut results = Vec::new();

    // ── 1. SNAPSHOT_EXISTS ───────────────────────────────────
    if input.snapshot_exists {
        results.push(ConditionResult {
            code: ConditionCode::SnapshotExists,
            status: ConditionStatus::Pass,
            summary: "READY snapshot available.".to_string(),
        });
    } else {
        results.push(ConditionResult {
            code: ConditionCode::SnapshotExists,
            status: ConditionStatus::Incomplete,
            summary: "No READY snapshot. Index the repo first.".to_string(),
        });
        // No snapshot → conditions 2-7 are not evaluated.
        return results;
    }

    // ── 2. INDEX_NOT_EMPTY ──────────────────────────────────
    if input.files_total > 0 {
        results.push(ConditionResult {
            code: ConditionCode::IndexNotEmpty,
            status: ConditionStatus::Pass,
            summary: format!("{} files indexed.", input.files_total),
        });
    } else {
        results.push(ConditionResult {
            code: ConditionCode::IndexNotEmpty,
            status: ConditionStatus::Incomplete,
            summary: "Snapshot has zero indexed files.".to_string(),
        });
    }

    // ── 3. UNPARSED_FILES (+ deprecated STALE_FILES alias) ───
    //
    // INDEX-BASIS-1: this condition measures PARSE status — files whose recorded
    // parse state is behind the stored file version (`get_stale_files`). It is NOT
    // working-tree drift (that is INDEX_DRIFT below). The old name `STALE_FILES`
    // implied "the tree has moved", which it never measured — a name/semantics
    // mismatch. The honest name is `UNPARSED_FILES`.
    //
    // The deprecated `STALE_FILES` condition is emitted alongside for one release
    // (same status + count) so any consumer keyed on the old code keeps working;
    // human output suppresses it (rgr). Both carry the SAME status, so the verdict
    // is unchanged by the duplication.
    let (parse_status, unparsed_summary) = if input.stale_file_count == 0 {
        (
            ConditionStatus::Pass,
            "No files failed to parse.".to_string(),
        )
    } else {
        (
            ConditionStatus::Fail,
            format!("{} files could not be parsed.", input.stale_file_count),
        )
    };
    results.push(ConditionResult {
        code: ConditionCode::UnparsedFiles,
        status: parse_status,
        summary: unparsed_summary.clone(),
    });
    results.push(ConditionResult {
        code: ConditionCode::StaleFiles,
        status: parse_status,
        summary: format!("[deprecated: renamed UNPARSED_FILES] {}", unparsed_summary),
    });

    // ── 3b. INDEX_DRIFT ─────────────────────────────────────
    //
    // INDEX-BASIS-1 §2.4: working-tree drift since the index basis commit.
    // Informational — `Incomplete` when the tree has moved (or the basis/drift is
    // unknown), `Pass` when clean or not-a-git-repo, NEVER `Fail` by itself. The
    // drift is computed by the daemon (git + storage) and handed in via
    // `input.index_drift`; when absent (the simple `run_check` entry), the
    // condition is OMITTED rather than fabricated as a false "unknown".
    if let Some(drift) = &input.index_drift {
        let status = if drift.makes_check_incomplete() {
            ConditionStatus::Incomplete
        } else {
            ConditionStatus::Pass
        };
        results.push(ConditionResult {
            code: ConditionCode::IndexDrift,
            status,
            summary: drift.describe(),
        });
    }

    // ── 4. CALL_GRAPH_RELIABILITY ───────────────────────────
    //
    // **POLICY NOTE:** MEDIUM -> pass is a check-specific
    // interpretation. The trust crate defines MEDIUM as
    // 50-85% resolution. Check treats this as "safe enough to
    // act on." This is NOT inherited from the trust contract.
    //
    // RELIABILITY-REFRAME-1: the summary speaks the reader's frame
    // ("your code's calls M% resolved (BAND)") from the ONE shared
    // projection, NOT "Call graph reliability is BAND" (which graded
    // repo-graph's own pipeline). The STATUS mapping is unchanged —
    // still HIGH/MEDIUM -> pass, LOW -> fail, None -> incomplete.
    //
    // review-3 §1: check consumes the FULL projection — REAL external share
    // (`total_calls` = all calls; `external` = all-unresolved minus in-scope-or-unclassified) + the
    // named coverage map — NOT an `external=0` placeholder. The named list fits check's
    // one-line-per-condition budget via the compact `named_coverage_map_line`, so this is
    // the full projection, not the operator's share+count floor.
    let total_calls = input.resolved_calls + input.unresolved_calls;
    let external = input
        .unresolved_calls
        .saturating_sub(input.unresolved_calls_internal_like);
    let view = CallReliabilityView::derive(
        input.resolved_calls,
        input.unresolved_calls_internal_like,
        external,
        total_calls,
        input.external_targets.clone(),
        input.call_graph_reliability,
    );
    // RELIABILITY-REFRAME-1 (review-1 §1): a repo with NO in-scope calls has nothing to
    // measure. `compute_call_graph_reliability(0,0)` returns a vacuous HIGH; treating that
    // as PASS would grade "no data" as reliable ("PASS: No in-scope calls measured"). The
    // shared projection already models this as `resolution == None`; act on it — no in-scope
    // calls → Incomplete (unknown), never Pass/Fail, regardless of the vacuous band. Only when
    // there IS an in-scope rate does the band drive the PASS/FAIL verdict.
    if view.resolution.is_none() {
        // iteration-5 §1: Incomplete status, but the FULL coverage map still renders — an
        // all-external repo keeps its external share + named targets + EY1-A bases (the same
        // projection orient/trust show), not a bare "no data" line. The verdict clause is the
        // shared `resolved_phrase()` ("no in-scope calls measured"); `append_call_graph_coverage`
        // adds the map and (correctly) skips the conservative caveat — with no in-scope
        // denominator there is nothing to qualify.
        let verdict = format!("{}.", sentence_case(&view.resolved_phrase()));
        results.push(ConditionResult {
            code: ConditionCode::CallGraphReliability,
            status: ConditionStatus::Incomplete,
            summary: append_call_graph_coverage(verdict, &view, input.unresolved_calls_unknown),
        });
    } else {
        // The band-verdict sentence (reader-frame, from the shared view) + PASS/FAIL status.
        let (status, verdict) = match input.call_graph_reliability {
            Some(AgentReliabilityLevel::High) => (
                ConditionStatus::Pass,
                format!("{}.", sentence_case(&view.resolved_with_band())),
            ),
            Some(AgentReliabilityLevel::Medium) => (
                ConditionStatus::Pass,
                format!("{} — advisory.", sentence_case(&view.resolved_with_band())),
            ),
            Some(AgentReliabilityLevel::Low) => (
                ConditionStatus::Fail,
                format!(
                    "{} — verify call/dead claims against source.",
                    sentence_case(&view.resolved_with_band())
                ),
            ),
            None => (
                ConditionStatus::Incomplete,
                "Your code's call resolution reliability is unavailable for this snapshot."
                    .to_string(),
            ),
        };
        results.push(ConditionResult {
            code: ConditionCode::CallGraphReliability,
            status,
            summary: append_call_graph_coverage(verdict, &view, input.unresolved_calls_unknown),
        });
    }

    // ── 5. ENRICHMENT_STATE ─────────────────────────────────
    // The reader-facing SUMMARY strings are shared consts (below), reused verbatim
    // by the `reliability` breakdown surface so the enrichment vocabulary has ONE
    // home (RESOLUTION-BREAKDOWN-CLI-1 review-0 F1). check keeps its own Pass/Fail/
    // Incomplete status mapping; only the wording is consolidated. Output is
    // byte-identical (the consts hold the exact prior literals).
    {
        let status = match input.enrichment_state {
            Some(EnrichmentState::Ran) | Some(EnrichmentState::NotApplicable) => {
                ConditionStatus::Pass
            }
            Some(EnrichmentState::NotRun) => ConditionStatus::Fail,
            None => ConditionStatus::Incomplete,
        };
        results.push(ConditionResult {
            code: ConditionCode::EnrichmentState,
            status,
            summary: enrichment_state_summary(input.enrichment_state).to_string(),
        });
    }

    // ── 7. GATE_STATUS ──────────────────────────────────────
    //
    // **POLICY NOTE:** NotConfigured -> pass is a check-specific
    // interpretation. "No policy = no violation." If the product
    // later wants policy-coverage as a concern, it would be a
    // separate condition code.
    match input.gate_outcome {
        Some(GateOutcomeForCheck::Pass) => {
            results.push(ConditionResult {
                code: ConditionCode::GateStatus,
                status: ConditionStatus::Pass,
                summary: "Gate passes.".to_string(),
            });
        }
        Some(GateOutcomeForCheck::Fail) => {
            results.push(ConditionResult {
                code: ConditionCode::GateStatus,
                status: ConditionStatus::Fail,
                summary: "Gate fails.".to_string(),
            });
        }
        Some(GateOutcomeForCheck::Incomplete) => {
            results.push(ConditionResult {
                code: ConditionCode::GateStatus,
                status: ConditionStatus::Incomplete,
                summary: "Gate incomplete: missing evidence.".to_string(),
            });
        }
        Some(GateOutcomeForCheck::NotConfigured) => {
            results.push(ConditionResult {
                code: ConditionCode::GateStatus,
                status: ConditionStatus::Pass,
                summary: "No gate policy configured.".to_string(),
            });
        }
        None => {
            results.push(ConditionResult {
                code: ConditionCode::GateStatus,
                status: ConditionStatus::Incomplete,
                summary: "Gate status data unavailable.".to_string(),
            });
        }
    }

    results
}

/// Cap on named external targets in `check`'s compact one-line coverage suffix; the
/// remainder summarise as "+N more" (`trust`/`orient` carry the fuller list). Tighter than
/// orient's map cap — check folds the map into a single per-condition summary line.
const CHECK_EXTERNAL_MAP_LIMIT: usize = 3;

/// Append the FULL reader-frame coverage projection (review-3 §1/§2) to `check`'s
/// CALL_GRAPH_RELIABILITY verdict: the external SHARE (or the honest "none identified" when
/// the heuristic matched zero externals), the named coverage map + its compact EY1-A basis,
/// and the conservative-rate caveat when the unclassified share is material. Every clause is
/// drawn from the SAME shared [`CallReliabilityView`] / `reliability` helpers that `orient`
/// and `trust` render, so check's coverage cannot fork from theirs — it is only COMPACTED to
/// check's single-line-per-condition budget. Each clause is emitted as its own sentence
/// (capitalised, period-terminated) after the verdict.
fn append_call_graph_coverage(
    verdict: String,
    view: &CallReliabilityView,
    unclassified: u64,
) -> String {
    let mut clauses: Vec<String> = Vec::new();
    if let Some(line) = view.external_line() {
        clauses.push(line);
    }
    if let Some(map) = view.named_coverage_map_line(CHECK_EXTERNAL_MAP_LIMIT) {
        clauses.push(map);
        clauses.push(reliability::COMPACT_HEURISTIC_BASES.to_string());
    }
    if let Some(res) = view.resolution {
        if let Some(caveat) =
            reliability::unclassified_caveat(unclassified, res.in_scope_or_unclassified_total)
        {
            clauses.push(caveat);
        }
    }

    let mut out = verdict;
    for clause in clauses {
        out.push(' ');
        out.push_str(&sentence_case(&clause));
        if !out.ends_with('.') {
            out.push('.');
        }
    }
    out
}
