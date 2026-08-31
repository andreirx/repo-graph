//! Phase 1: Condition evaluation.
//!
//! A pure function that takes `CheckInput` and produces a
//! `Vec<ConditionResult>`, one per condition code. When no
//! snapshot exists, only `SNAPSHOT_EXISTS` is evaluated (with
//! status Incomplete); conditions 2-7 are omitted entirely.

use crate::dto::ceiling_fact::CeilingFact;
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
/// `EnrichmentState::InFlight` — a background enrichment pass is queued/running for this snapshot
/// RIGHT NOW (ORIENT-FACT-COHERENCE-1). Reader-frame: figures may still rise, re-run when it
/// completes. Suppresses the stale "run `rmap enrich`" CTA — never render a "did not run" for a phase
/// that is executing. The substring "in progress" is what the reproducing test keys on.
pub const ENRICHMENT_SUMMARY_IN_FLIGHT: &str =
    "Enrichment pass in progress — resolution figures may rise; re-run when it completes.";
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
        Some(EnrichmentState::InFlight) => ENRICHMENT_SUMMARY_IN_FLIGHT,
        None => ENRICHMENT_SUMMARY_UNAVAILABLE,
    }
}

/// The machine wire token for the enrichment state — the SAME snake_case tokens the
/// `repo_graph_enrichment` crate serializes (`ran`/`not_run`/`not_applicable`), so the
/// `reliability` JSON surface speaks the established vocabulary. `in_flight` is the additive token for
/// the daemon-injected in-flight state (ORIENT-FACT-COHERENCE-1). `None` = unavailable
/// (serialized as JSON `null`, never a fabricated token).
pub fn enrichment_state_token(state: Option<EnrichmentState>) -> Option<&'static str> {
    match state {
        Some(EnrichmentState::Ran) => Some("ran"),
        Some(EnrichmentState::NotApplicable) => Some("not_applicable"),
        Some(EnrichmentState::NotRun) => Some("not_run"),
        Some(EnrichmentState::InFlight) => Some("in_flight"),
        None => None,
    }
}

/// The reader display list of ceilinged languages joined for a sentence: `"C"`, `"C/C++"`,
/// `"Python"`. The SINGLE definition of the ceiling-language separator, shared by the
/// CALL_GRAPH_RELIABILITY verdict and the ENRICHMENT_STATE non-failing form (its two callers below).
fn ceiling_language_list(languages: &[String]) -> String {
    languages.join("/")
}

/// CHECK-SIGNAL-1: append the unknown-WITH-REASON clause when the daemon's capability read failed
/// ([`CeilingFact::Unknown`]) on a DEGRADING call-graph condition. The status stays failing — a read
/// failure may never mint a Pass (Fact Certainty Model) — but the unknown is made VISIBLE with its
/// reason (STANDING HONESTY RULE #1: a classified fallible read is never swallowed to a sentinel).
/// `reason` is the daemon read error, carried in-band (never stderr-only, per operator ruling).
fn append_ceiling_unknown(mut summary: String, reason: &str) -> String {
    summary.push(' ');
    summary.push_str(&format!(
        "Whether this is a permanent no-resolver ceiling is unknown ({reason}); \
         treated as actionable pending that capability read."
    ));
    summary
}

/// CHECK-SIGNAL-1: build the `CALL_GRAPH_RELIABILITY` condition, honouring the daemon-injected
/// call-graph capability fact ([`CeilingFact`], possibly not supplied → `None`).
///
/// The fact modulates ONLY a DEGRADING condition (LOW, or no in-scope calls to measure); a
/// MEDIUM/HIGH condition passes on its own figures, so the fact is not consulted there. On a
/// degrading condition (exhaustive over the sum — a new capability outcome must break this match):
///   - [`CeilingFact::Ceiling`] → the LOW / no-in-scope figure is this build's deterministic-
///     extraction ceiling, not an actionable gap, so the condition PASSES as a stated limitation
///     naming the languages, with `ceiling = true`. FIGURES render UNCHANGED (§2.1); only the
///     status/wording change.
///   - [`CeilingFact::Unknown`] → the capability read failed: keep the pre-slice FAILING
///     classification (never a false Pass) and surface the reason (`append_ceiling_unknown`).
///   - [`CeilingFact::NoCeiling`] / not supplied (`None`) → the pre-CHECK-SIGNAL-1 logic,
///     byte-identical.
///
/// A repo that is `None`-reliability with in-scope calls present stays the ordinary "reliability
/// unavailable" Incomplete (a data-availability unknown, NOT the resolver ceiling) — it is not
/// degrading, so the fact is not consulted.
fn evaluate_call_graph_reliability(
    input: &CheckInput,
    view: &CallReliabilityView,
    ceiling_fact: Option<&CeilingFact>,
) -> ConditionResult {
    // RELIABILITY-REFRAME-1 (review-1 §1): a repo with NO in-scope calls has nothing to
    // measure. `compute_call_graph_reliability(0,0)` returns a vacuous HIGH; treating that
    // as PASS would grade "no data" as reliable. The shared projection models this as
    // `resolution == None`; act on it — no in-scope calls → Incomplete (unknown), never
    // Pass/Fail, regardless of the vacuous band. Only when there IS an in-scope rate does the
    // band drive the PASS/FAIL verdict.
    let is_low = matches!(
        input.call_graph_reliability,
        Some(AgentReliabilityLevel::Low)
    );
    let is_degrading = view.resolution.is_none() || is_low;

    // CHECK-SIGNAL-1: decide how the capability fact modulates a DEGRADING condition. Exhaustive
    // over `CeilingFact` (no wildcard arm on the sum — operator ruling `ceiling-read-unknown`):
    // `Ceiling` early-returns the passing stated limitation; `Unknown` yields its reason to append
    // after the pre-slice classification; `NoCeiling`/not-supplied yield nothing (pre-slice,
    // byte-identical).
    let unknown_reason: Option<&str> = if is_degrading {
        match ceiling_fact {
            Some(CeilingFact::Ceiling { languages }) => {
                let langs = ceiling_language_list(languages);
                // The figure clause reads the SAME shared projection the pre-slice path renders —
                // the rate ("your code's calls N% resolved") when there is one, or the honest
                // no-measurement phrasing when there are no in-scope calls — so the FIGURES are
                // byte-for-byte what the reader would otherwise see; only the framing is the ceiling.
                let figure = if view.resolution.is_some() {
                    format!(
                        "{} is the deterministic-extraction figure",
                        view.resolved_phrase()
                    )
                } else {
                    "no in-scope calls to resolve on this build".to_string()
                };
                let verdict = format!(
                    "Call-graph resolution has reached this build's ceiling for {langs} \
                     (no resolver exists) — {figure}; verify call/dead claims against source."
                );
                return ConditionResult {
                    code: ConditionCode::CallGraphReliability,
                    status: ConditionStatus::Pass,
                    summary: append_call_graph_coverage(
                        verdict,
                        view,
                        input.unresolved_calls_unknown,
                    ),
                    ceiling: true,
                };
            }
            Some(CeilingFact::Unknown { reason }) => Some(reason.as_str()),
            Some(CeilingFact::NoCeiling) | None => None,
        }
    } else {
        None
    };

    // ── Pre-CHECK-SIGNAL-1 classification (unchanged, byte-identical for NoCeiling/not-supplied). ──
    let (status, verdict) = if view.resolution.is_none() {
        // iteration-5 §1: Incomplete status, but the FULL coverage map still renders — an
        // all-external repo keeps its external share + named targets + EY1-A bases (the same
        // projection orient/trust show), not a bare "no data" line.
        (
            ConditionStatus::Incomplete,
            format!("{}.", sentence_case(&view.resolved_phrase())),
        )
    } else {
        // The band-verdict sentence (reader-frame, from the shared view) + PASS/FAIL status.
        match input.call_graph_reliability {
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
        }
    };
    let mut summary = append_call_graph_coverage(verdict, view, input.unresolved_calls_unknown);
    // A degrading condition on an UNKNOWN capability renders the unknown WITH its reason, while
    // keeping the failing status above (never a false Pass, never a swallowed read).
    if let Some(reason) = unknown_reason {
        summary = append_ceiling_unknown(summary, reason);
    }
    ConditionResult {
        code: ConditionCode::CallGraphReliability,
        status,
        summary,
        ceiling: false,
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
            ceiling: false,
        });
    } else {
        results.push(ConditionResult {
            code: ConditionCode::SnapshotExists,
            status: ConditionStatus::Incomplete,
            summary: "No READY snapshot. Index the repo first.".to_string(),
            ceiling: false,
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
            ceiling: false,
        });
    } else {
        results.push(ConditionResult {
            code: ConditionCode::IndexNotEmpty,
            status: ConditionStatus::Incomplete,
            summary: "Snapshot has zero indexed files.".to_string(),
            ceiling: false,
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
        ceiling: false,
    });
    results.push(ConditionResult {
        code: ConditionCode::StaleFiles,
        status: parse_status,
        summary: format!("[deprecated: renamed UNPARSED_FILES] {}", unparsed_summary),
        ceiling: false,
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
            ceiling: false,
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
    // CHECK-SIGNAL-1: the daemon-injected call-graph capability fact (one source; §2.1) modulates a
    // DEGRADING resolution: `Ceiling` → passing stated limitation (figures unchanged), `Unknown` →
    // keep failing + surface the reason, `NoCeiling`/not-supplied → pre-slice, byte-identical. The
    // reliability FIGURES are untouched in every case: only the classification/wording change.
    let ceiling_fact = input.ceiling_fact.as_ref();
    results.push(evaluate_call_graph_reliability(input, &view, ceiling_fact));

    // ── 5. ENRICHMENT_STATE ─────────────────────────────────
    // The reader-facing SUMMARY strings are shared consts (below), reused verbatim
    // by the `reliability` breakdown surface so the enrichment vocabulary has ONE
    // home (RESOLUTION-BREAKDOWN-CLI-1 review-0 F1). check keeps its own Pass/Fail/
    // Incomplete status mapping; only the wording is consolidated. Output is
    // byte-identical (the consts hold the exact prior literals).
    {
        // CHECK-SIGNAL-1 (§2.2): on a PERMANENT-ceiling repo, "Enrichment phase did not run" is a
        // FALSE failure — there is no resolver to run for the materially-present language(s), so the
        // reader can do nothing about it (django's measured defect vs leveldb's honest "no eligible
        // edges" Pass). Reclassify THAT ONE case (NotRun × CeilingFact::Ceiling) to the honest
        // non-failing form, naming the ceilinged languages — driven by the SAME capability fact as
        // the call-graph condition (one fact, both surfaces). Every other state (Ran / NotApplicable
        // / InFlight / None, and NotRun on a NoCeiling / Unknown / not-supplied repo) is unchanged,
        // byte-identical: a NotRun on a genuinely enrichable repo IS actionable; and on a truly
        // ceilinged repo the CALL_GRAPH_RELIABILITY is always degrading, so an Unknown capability is
        // surfaced there (the only site where it is material) — never swallowed.
        //
        // The inner match on `CeilingFact` is exhaustive (no wildcard on the sum — operator ruling
        // `ceiling-read-unknown`): a new capability outcome must break this site too.
        let ceiling_langs_for_enrichment: Option<String> =
            match (input.enrichment_state, input.ceiling_fact.as_ref()) {
                (Some(EnrichmentState::NotRun), Some(fact)) => match fact {
                    CeilingFact::Ceiling { languages } => Some(ceiling_language_list(languages)),
                    CeilingFact::NoCeiling | CeilingFact::Unknown { .. } => None,
                },
                _ => None,
            };
        match ceiling_langs_for_enrichment {
            Some(langs) => {
                results.push(ConditionResult {
                    code: ConditionCode::EnrichmentState,
                    status: ConditionStatus::Pass,
                    summary: format!(
                        "No semantic-resolution path exists for {langs} on this build; \
                         enrichment does not apply."
                    ),
                    ceiling: true,
                });
            }
            None => {
                let status = match input.enrichment_state {
                    // ORIENT-FACT-COHERENCE-1: an in-flight pass is an honest NON-FAILING form
                    // (parallel to NotApplicable's "no eligible edges" Pass) — check must not FAIL a
                    // handoff because a pass that would raise the figures is still running; the
                    // summary carries the truth.
                    Some(EnrichmentState::Ran)
                    | Some(EnrichmentState::NotApplicable)
                    | Some(EnrichmentState::InFlight) => ConditionStatus::Pass,
                    Some(EnrichmentState::NotRun) => ConditionStatus::Fail,
                    None => ConditionStatus::Incomplete,
                };
                results.push(ConditionResult {
                    code: ConditionCode::EnrichmentState,
                    status,
                    summary: enrichment_state_summary(input.enrichment_state).to_string(),
                    ceiling: false,
                });
            }
        }
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
                ceiling: false,
            });
        }
        Some(GateOutcomeForCheck::Fail) => {
            results.push(ConditionResult {
                code: ConditionCode::GateStatus,
                status: ConditionStatus::Fail,
                summary: "Gate fails.".to_string(),
                ceiling: false,
            });
        }
        Some(GateOutcomeForCheck::Incomplete) => {
            results.push(ConditionResult {
                code: ConditionCode::GateStatus,
                status: ConditionStatus::Incomplete,
                summary: "Gate incomplete: missing evidence.".to_string(),
                ceiling: false,
            });
        }
        Some(GateOutcomeForCheck::NotConfigured) => {
            results.push(ConditionResult {
                code: ConditionCode::GateStatus,
                status: ConditionStatus::Pass,
                summary: "No gate policy configured.".to_string(),
                ceiling: false,
            });
        }
        None => {
            results.push(ConditionResult {
                code: ConditionCode::GateStatus,
                status: ConditionStatus::Incomplete,
                summary: "Gate status data unavailable.".to_string(),
                ceiling: false,
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
