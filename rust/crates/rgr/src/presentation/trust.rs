//! Presentation layer for the `trust` command.
//!
//! # CLI-OUT-2B → TRUST-LIVEGRAPH-IMPL
//!
//! The daemon now returns the ratified HYBRID `CoherenceEnvelope<CoherentTrustReport>` (the wrapper is the
//! top level). The `--json` path prints it verbatim; this human path projects it. The render is the honest
//! two-half model (`docs/slices/trust-livegraph-1.md` §3e / W2):
//!   - a NEW **Current-State Posture** section (Half A, `source = livegraph`) — residency / per-partition
//!     freshness / language / producer / migrated-answer capability, GENUINELY served from the LiveGraph;
//!   - the existing v1 sections (Half B, `source = sqlite`) with their bullet text BYTE-IDENTICAL to the
//!     pre-wrapper report, each heading carrying a `(source, snapshot-scoped extraction, freshness)` label so
//!     a reader never mistakes the OUTGOING-extractor snapshot diagnostics for the current-state LiveGraph
//!     resolution (F5);
//!   - an overall **Posture** line carrying the root MEET (current-state-vs-snapshot freshness).
//!
//! The renderer reuses the trust crate's coherent DTOs (`repo_graph_trust::CoherentTrustReport`) for
//! deserialization so the CLI view cannot silently drift from the daemon's wire shape.
//!
//! ## Human Output Structure
//!
//! ```text
//! Trust Report: billing-service
//! Snapshot: snap_01kr...
//! Posture: Exact (Fresh)
//!
//! Current-State Posture  (livegraph, current-state, Fresh)
//!   - Resident: yes (1 partition)
//!   - app: Fresh, TypeScript, producer scip-typescript@0.4.0
//!   - Producer available: yes
//!   - Migrated-answer capability: yes
//!
//! Resolution  (sqlite, snapshot-scoped extraction, Fresh)
//!   - your code's calls 78% resolved (1234 of 1582 in-scope or unclassified)
//!   - 12% of calls go into external libraries — follow to their crates/docs
//!   - Edges: 95% resolved (2100 of 2210)
//!
//! Reliability  (sqlite, snapshot-scoped extraction, Fresh)
//!   - Call-graph: LOW (your code's calls 22% resolved (below 50% target))
//!   ...
//! ```
//!
//! RELIABILITY-REFRAME-1: the Resolution "your code's calls" line is the IN-SCOPE
//! rate (external-library calls excluded from the denominator) — the prior "Calls:
//! X% resolved" bullet recomputed it external-INCLUSIVE, grading repo-graph's own
//! pipeline. The reader-frame vocabulary + derivation is the ONE shared projection
//! [`repo_graph_agent::reliability::CallReliabilityView`] (also consumed by `orient`
//! and `check`), so no surface can re-derive a divergent number.

use repo_graph_agent::attribution;
use repo_graph_agent::dto::ceiling_fact::CeilingReport;
use repo_graph_agent::reliability::{self, CallReliabilityView, ExternalTarget};
use repo_graph_coherence::{AnswerClass, CoherenceEnvelope, FreshnessState, Provenance, Source};
use repo_graph_trust::types::{ReliabilityAxisScore, ReliabilityLevel};
use repo_graph_trust::{CoherentTrustReport, LiveGraphPosture};

use crate::presentation::{bullet, heading, kv_line};

/// The wire type `rmap trust` deserializes: the daemon's `CoherenceEnvelope<CoherentTrustReport>`.
pub type TrustEnvelope = CoherenceEnvelope<CoherentTrustReport>;

// ── Labels (the per-section source/freshness honesty markers) ───────────────────────────────────

fn freshness_label(f: FreshnessState) -> &'static str {
    match f {
        FreshnessState::Fresh => "Fresh",
        FreshnessState::Stale => "Stale",
        FreshnessState::PrecisionPending => "PrecisionPending",
        FreshnessState::RefreshFailed => "RefreshFailed",
        FreshnessState::Unavailable => "Unavailable",
    }
}

fn class_label(c: AnswerClass) -> &'static str {
    match c {
        AnswerClass::Exact => "Exact",
        AnswerClass::Partial => "Partial",
        AnswerClass::Unavailable => "Unavailable",
        AnswerClass::Stale => "Stale",
    }
}

fn source_label(p: &Provenance) -> String {
    let parts: Vec<&str> = p
        .source
        .iter()
        .map(|s| match s {
            Source::Livegraph => "livegraph",
            Source::Sqlite => "sqlite",
            Source::Filesystem => "filesystem",
            Source::Declaration => "declaration",
        })
        .collect();
    parts.join("+")
}

/// A labelled section heading: `Title  (source, scope, Freshness)`. `scope` is `"snapshot-scoped
/// extraction"` for Half-B residual leaves and `"current-state"` for the Half-A posture leaf.
fn labelled_heading<T>(title: &str, leaf: &CoherenceEnvelope<T>, scope: &str) -> String {
    heading(&format!(
        "{}  ({}, {}, {})",
        title,
        source_label(&leaf.provenance),
        scope,
        freshness_label(leaf.freshness)
    ))
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

// ── Top-level render ─────────────────────────────────────────────────────────────────────────────

/// Render trust's coherence-wrapped daemon response (TRUST-LIVEGRAPH-IMPL).
pub fn render_trust_envelope(env: &TrustEnvelope) -> String {
    let v = &env.value;
    let mut out = String::new();

    // ── Header ──
    let repo_display = v.display_name.as_deref().unwrap_or(&v.snapshot_uid);
    out.push_str(&kv_line("Trust Report", repo_display));
    out.push_str(&kv_line("Snapshot", &truncate_uid(&v.snapshot_uid)));
    // The overall MEET posture (root): the hybrid's honest current-state-vs-snapshot freshness. A Fresh
    // LiveGraph posture over a Stale snapshot reads Stale; a cold LiveGraph reads Unavailable (D-T6).
    out.push_str(&kv_line(
        "Posture",
        &format!(
            "{} ({})",
            class_label(env.trust.class),
            freshness_label(env.freshness)
        ),
    ));
    out.push('\n');

    // ── Half A — Current-State Posture (livegraph) ──
    out.push_str(&render_posture(&v.current_state_posture));
    out.push('\n');

    // ── Half B — residual extraction diagnostics (sqlite, snapshot-scoped) ──
    out.push_str(&render_resolution(v));
    out.push('\n');

    out.push_str(&render_reliability(v));
    out.push('\n');

    // RELIABILITY-REFRAME-1 (review-1 §3) removed the raw "Unresolved Breakdown"
    // (`calls_obj_method_needs_type_info`, …) and "Classification" (`external_library_candidate`,
    // `internal_candidate`, …) sections: they narrated OUR extraction pipeline in raw internal
    // vocabulary — noise to the reader. ATTRIBUTION-1 (slice §1.3) now REFRAMES the classification
    // breakdown into the reader's frame ("external library / dependency: 30 references — follow to
    // their crate/package docs") via the ONE shared mapping (`repo_graph_agent::attribution`) — the
    // reader learns WHERE their unresolved references go without ever seeing a classifier code. The
    // raw `categories` breakdown (extraction failure modes, not attribution) stays OFF the human
    // surface; both `categories` + `classifications` leaves remain on the STRUCTURED `--json`
    // surface verbatim (the debug diagnostic surface), so no fact is lost.
    let attribution_breakdown = render_unresolved_attribution(v);
    if !attribution_breakdown.is_empty() {
        out.push_str(&attribution_breakdown);
        out.push('\n');
    }

    // RECON-M-R4 (§5.5): the Layer-2 landing on the "where they go" surface — unresolved calls the
    // compiler resolved (likely resolutions) + contested resolutions (syntax vs compiler disagree).
    // ABSENT on zero-SCIP / no-hint repos (the daemon omits the field; renders nothing on None).
    let layer2 = crate::presentation::witnesses::render_layer2_resolution_section(
        v.layer2_resolution.as_ref(),
    );
    if !layer2.is_empty() {
        out.push_str(&layer2);
        out.push('\n');
    }

    // EY1-A: the likely-external receiver-call orientation projection (Layer-2, read-side).
    let external_receivers = render_enrichment_external(v);
    if !external_receivers.is_empty() {
        out.push_str(&external_receivers);
        out.push('\n');
    }

    // RECON-M-R3a: the additive Witnesses section (union accounting / divergence posture) —
    // rendered by the SHARED client-side witness projection; ABSENT entirely on zero-SCIP repos
    // (R-0: the daemon omits the field; this renders nothing on None).
    let witnesses = crate::presentation::witnesses::render_trust_section(v.witnesses.as_ref());
    if !witnesses.is_empty() {
        out.push_str(&witnesses);
        out.push('\n');
    }

    let suspicious = render_suspicious_modules(v);
    if !suspicious.is_empty() {
        out.push_str(&suspicious);
        out.push('\n');
    }

    let downgrades = render_downgrades(v);
    if !downgrades.is_empty() {
        out.push_str(&downgrades);
        out.push('\n');
    }

    let caveats = &v.caveats.value;
    if !caveats.is_empty() {
        out.push_str(&labelled_heading(
            "Caveats",
            &v.caveats,
            "snapshot-scoped extraction",
        ));
        for caveat in caveats {
            out.push_str(&bullet(caveat));
        }
        out.push('\n');
    }

    out.trim_end().to_string()
}

// ── Half A — Current-State Posture (livegraph) ──────────────────────────────────────────────────

fn render_posture(leaf: &CoherenceEnvelope<LiveGraphPosture>) -> String {
    let p = &leaf.value;
    let mut out = labelled_heading("Current-State Posture", leaf, "current-state");

    if !p.resident {
        // M-R3A-TRUST-POSTURE (ratified 2026-07-19): the legacy `resident` field is the SERVE
        // fact; the amendment fields carry the two distinguished facts. A resident-but-withheld
        // state must NEVER read as "not loaded" (the review-0 contradiction — a false state
        // claim beside a W-BOTH witnesses block).
        if p.livegraph_resident == Some(true) && p.coherent_serve_eligible == Some(false) {
            out.push_str(&bullet(
                "Resident: yes (compiler analysis is loaded) — current-state detail withheld: \
                 not verified coherent with this report's snapshot for this request",
            ));
            return out;
        }
        out.push_str(&bullet(
            "Resident: no (LiveGraph not loaded for this repo — current-state posture unavailable)",
        ));
        return out;
    }

    let n = p.partitions.len();
    out.push_str(&bullet(&format!(
        "Resident: yes ({} partition{})",
        n,
        if n == 1 { "" } else { "s" }
    )));
    for part in &p.partitions {
        let lang = if part.typescript_primary {
            "TypeScript"
        } else {
            "non-TypeScript"
        };
        let producer = if part.producer_fingerprint.is_empty() {
            "(no producer)".to_string()
        } else {
            part.producer_fingerprint.clone()
        };
        out.push_str(&bullet(&format!(
            "{}: {}, {}, producer {}",
            part.partition_id,
            freshness_label(part.freshness),
            lang,
            producer
        )));
    }
    out.push_str(&bullet(&format!(
        "Producer available: {}",
        yes_no(p.producer_available)
    )));
    out.push_str(&bullet(&format!(
        "Migrated-answer capability: {}",
        yes_no(p.migrated_answer_capability)
    )));
    out
}

// ── Half B — residual diagnostics sections (bullet text byte-identical to the v1 report) ────────

fn render_resolution(v: &CoherentTrustReport) -> String {
    // Labelled by the resolution leaf (sqlite, snapshot-scoped). The Edges line carries the v1
    // `edges_resolved == edges_total` quirk verbatim (RISK-T-F).
    let mut out = labelled_heading("Resolution", &v.resolution, "snapshot-scoped extraction");
    let r = &v.resolution.value;

    // RELIABILITY-REFRAME-1: the ONE shared projection. In-scope rate = resolved over in-scope
    // references only; external-library calls are out of source scope (unresolvable by design) and
    // are EXCLUDED from the denominator (`unresolved_calls_internal_like`). The prior code recomputed
    // `resolved / (resolved + unresolved_calls)` external-INCLUSIVE — grading repo-graph's own
    // pipeline coverage instead of the reader's code. The excluded external share is named on its own
    // line below (context, not a grade). The band (Reliability section) is scored on this same in-scope
    // denominator too — genuine in-scope failure still reads low.
    // TRUST-FIRSTPARTY-1 (spec §2.3): the reader's external SHARE must EXCLUDE first-party
    // (repo-own workspace) calls so "external" means external. `first_party_calls` is the
    // CALLS-family subset storage counted — normally a strict subset of `unresolved_calls_external`.
    // The FROZEN in-scope resolution rate is derived from `unresolved_calls_internal_like` and is
    // NOT a function of the external count, so — as `render_reliability` does — the resolution view
    // is built with `external = 0` (only `.resolution` is read from it); the external SHARE is
    // derived and rendered on its own below.
    let first_party_calls = v.external_dependencies.value.first_party_calls;
    let total_calls = r.resolved_calls + r.unresolved_calls;
    let view = CallReliabilityView::derive(
        r.resolved_calls,
        r.unresolved_calls_internal_like,
        0,
        total_calls,
        Vec::new(), // the named targets render in their own section below
        None,       // the band renders in the Reliability section, not here
    );
    // Honest 0-of-0 (slice §3 / REVISE #3): no in-scope calls is UNKNOWN, never a fabricated 100%.
    // review-3 §2: the denominator is "in-scope OR unclassified" (external-EXCLUDED, but it still
    // includes `unknown` classifications) — NOT known-internal. The label says so.
    match &view.resolution {
        Some(res) => out.push_str(&bullet(&format!(
            "{} ({} of {} in-scope or unclassified)",
            reliability::resolved_phrase_pct(res.pct),
            res.resolved,
            res.in_scope_or_unclassified_total
        ))),
        None => out.push_str(&bullet(reliability::NO_IN_SCOPE_CALLS)),
    }
    // The external SHARE, first-party-EXCLUDED (spec §2.3). `checked_sub`, NOT `saturating_sub`
    // (review-1 §2): `first_party_calls` is normally a subset of `unresolved_calls_external`, but if
    // the subset invariant does NOT hold (a corrupt or cross-version snapshot), saturating to 0
    // would FABRICATE a measured "no external calls". Instead the figure is UNKNOWN, rendered with a
    // reader-facing reason (STANDING HONESTY RULE 1 / architecture rule 6).
    match r.unresolved_calls_external.checked_sub(first_party_calls) {
        Some(external_for_share) => {
            // Reuse the reader-frame external wording via a view carrying the corrected count.
            let external_view = CallReliabilityView::derive(
                r.resolved_calls,
                r.unresolved_calls_internal_like,
                external_for_share,
                total_calls,
                Vec::new(),
                None,
            );
            // When the heuristic identified ZERO external calls (but calls exist) this reads "no
            // external-library calls identified (heuristic)" — a heuristic finding, never a
            // fabricated "0% external" or a silent omission (review-3 §2).
            if let Some(line) = external_view.external_line() {
                out.push_str(&bullet(&line));
            }
            // TRUST-FIRSTPARTY-1 (spec §2.3): when the repo's OWN workspace crates were among the
            // external-import calls, state the external/first-party split inline so the corrected
            // external figure is never read as a contradiction (CONTRADICTION-SWEEP-1 pattern).
            // Only rendered when there IS a split — repos without repo-own workspace references
            // stay byte-identical.
            if first_party_calls > 0 {
                out.push_str(&bullet(&reliability::external_first_party_split_line(
                    external_for_share,
                    first_party_calls,
                )));
            }
        }
        None => {
            // Invariant violated (first-party calls exceed the external-import total): the external
            // share cannot be honestly computed. Unknown WITH REASON, never a saturated zero.
            out.push_str(&bullet(&reliability::external_share_unreconciled_line(
                r.unresolved_calls_external,
                first_party_calls,
            )));
        }
    }
    // review-3 §2 (slice §2 degraded path): when a MATERIAL share of the denominator is
    // unclassified, the rate is a conservative lower bound — say so, in the reader's frame.
    if let Some(res) = view.resolution {
        if let Some(caveat) = reliability::unclassified_caveat(
            r.unresolved_calls_unknown,
            res.in_scope_or_unclassified_total,
        ) {
            out.push_str(&bullet(&caveat));
        }
    }

    let edge_pct = if r.edges_total > 0 {
        (r.edges_resolved as f64 / r.edges_total as f64 * 100.0).round() as u64
    } else {
        100
    };
    out.push_str(&bullet(&format!(
        "Edges: {}% resolved ({} of {})",
        edge_pct, r.edges_resolved, r.edges_total
    )));

    out
}

fn render_reliability(v: &CoherentTrustReport) -> String {
    let mut out = labelled_heading("Reliability", &v.reliability, "snapshot-scoped extraction");
    let r = &v.reliability.value;

    // RELIABILITY-REFRAME-1 (review-1 §1 / review-2 §2): the zero-denominator decision comes from
    // the ONE shared projection — `resolution.is_none()`, the SAME decision `ResolvedRate::
    // derive` makes for the Resolution line above — never a bespoke `== 0` here. The Call-graph band
    // is scored on the in-scope denominator, and `compute_call_graph_reliability(0,0)` is a vacuous
    // HIGH. Rendering "Call-graph: HIGH" for a repo with NO in-scope calls would grade "nothing to
    // measure" as reliable. Honest: no in-scope calls → "no in-scope calls measured" (unknown),
    // never a band. The counts are the resolution leaf's own denominator (the one the band uses).
    let res = &v.resolution.value;
    let view = CallReliabilityView::derive(
        res.resolved_calls,
        res.unresolved_calls_internal_like,
        0,
        res.resolved_calls + res.unresolved_calls_internal_like,
        Vec::new(),
        None,
    );
    // COHERENCE-POLISH-1 §2: the daemon-injected call-graph ceiling capability fact modulates ONLY a
    // DEGRADING call-graph condition — the SAME gate `check`'s `evaluate_call_graph_reliability` uses
    // (`view.resolution.is_none() || band == LOW`). This is the coherence the slice demands: trust
    // renders the ceiling posture in exactly the cases "check says the ceiling is reached", never on a
    // MEDIUM/HIGH band (where check passes on the figure and says nothing about a ceiling — the
    // homegrown extractor still resolved the calls it saw). On a degrading condition, a permanent
    // no-resolver ceiling suppresses "below N% target" (a target the reader can never approach) and a
    // ceiling sentence states WHY; a failed/unreadable capability fact renders unknown-WITH-REASON
    // (STANDING HONESTY RULE 1), never a softened posture and never silently swallowed.
    let ceiling = parse_call_graph_ceiling(v);
    let is_degrading =
        view.resolution.is_none() || matches!(r.call_graph.level, ReliabilityLevel::LOW);
    let at_ceiling = is_degrading && matches!(ceiling, Some(Ok(CeilingReport::Ceiling { .. })));

    if view.resolution.is_none() {
        out.push_str(&bullet(&format!(
            "Call-graph: {}",
            reliability::NO_IN_SCOPE_CALLS
        )));
    } else {
        out.push_str(&bullet(&format_axis_maybe_ceiling(
            "Call-graph",
            &r.call_graph,
            at_ceiling,
        )));
    }
    out.push_str(&bullet(&format_axis("Import-graph", &r.import_graph)));
    out.push_str(&bullet(&format_axis("Change-impact", &r.change_impact)));

    // The ceiling posture rides BELOW the axes so it frames the whole call-graph section — but ONLY on
    // a degrading condition (coherent with check; a MEDIUM/HIGH band passes on its own figure).
    if is_degrading {
        match ceiling {
            Some(Ok(CeilingReport::Ceiling { languages })) => {
                out.push_str(&bullet(&reliability::call_graph_ceiling_note(&languages)));
            }
            Some(Ok(CeilingReport::Unknown { reason })) => {
                out.push_str(&bullet(&reliability::call_graph_ceiling_unknown_note(
                    &reason,
                )));
            }
            // A PRESENT but unparseable field is a fact we HAVE yet cannot read — unknown-with-reason,
            // never swallowed (STANDING HONESTY RULE 1).
            Some(Err(reason)) => {
                out.push_str(&bullet(&reliability::call_graph_ceiling_unknown_note(
                    &reason,
                )));
            }
            // NoCeiling (or the field absent on an older daemon) → no ceiling posture, byte-identical.
            Some(Ok(CeilingReport::NoCeiling)) | None => {}
        }
    }

    out
}

/// COHERENCE-POLISH-1 §2: read the daemon-injected `call_graph_ceiling` field (the serialized
/// `CeilingReport`). `None` = the field is ABSENT (older daemon / the pure fold never attached it) —
/// legitimate absence, byte-identical existing output. `Some(Ok(_))` = the parsed capability fact.
/// `Some(Err(reason))` = the field is PRESENT but does not deserialize (a daemon↔CLI shape skew) — a
/// fact we have yet cannot read, surfaced unknown-WITH-REASON by the caller rather than swallowed
/// (STANDING HONESTY RULE 1: never `.ok()` a classified fallible parse whose result is rendered).
fn parse_call_graph_ceiling(v: &CoherentTrustReport) -> Option<Result<CeilingReport, String>> {
    let raw = v.call_graph_ceiling.as_ref()?;
    Some(serde_json::from_value::<CeilingReport>(raw.clone()).map_err(|e| e.to_string()))
}

// RELIABILITY-REFRAME-1 (review-1 §3): `render_unresolved_breakdown` + `render_classification`
// were removed from the HUMAN render — they emitted raw pipeline vocabulary ("Unresolved
// Breakdown", `external_library_candidate`, `internal_candidate`, `calls_obj_method_needs_type_info`)
// that grades OUR extractor, not the reader's code. The facts survive on the `--json` surface
// (`CoherentTrustReport.categories` / `.classifications` are serialized verbatim). See the note at
// their former call site in `render_trust_envelope`.

/// ATTRIBUTION-1 (slice §1.1/§1.3; review-1 REVISE #1/#2): the reader-frame reframe of
/// the unresolved-reference breakdown — the successor to the removed raw
/// `render_classification`.
///
/// Where the old section listed `external_library_candidate  30` (grading OUR classifier),
/// this NAMES where the reader's unresolved references go, through the ONE shared module
/// [`attribution`] (vocabulary AND the typed `basis_code → class` match, both in `agent`
/// per ATTR1-MAPPING-BOUNDARY option A), so the wording cannot fork across renderers.
/// Library calls are named per DECLARED dependency — "library call → serde: 12 references" —
/// from the `external_dependencies` provenance-join leaf; every other class renders its total.
///
/// It reads the FINER `basis_classifications` leaf (not the coarse `classifications`): the
/// 4-value classification folds third-party dependencies, the standard library, and runtime
/// globals into one `external_library_candidate` bucket, which cannot produce the reader's
/// distinct classes (review-0 #1/#2). BOTH the `classifications` and `basis_classifications`
/// leaves stay on the `--json` debug surface verbatim; the raw codes never reach the human.
/// Zero-count classes are skipped; an empty / absent aggregate renders nothing.
///
/// An unrecognized wire basis code (an older/newer daemon carrying a code this build predates)
/// folds into the honest [`attribution::OTHER_UNRESOLVED_LABEL`] bucket — the count is
/// preserved and the raw code never surfaced (the runtime analogue of the compile-time
/// exhaustiveness the typed mapping guarantees for known codes). The heuristic + provenance
/// honesty (declared-dependency identity across the three external-import bases; versions not
/// recorded; Java/Gradle limited — review-0 #3) rides the basis markers.
fn render_unresolved_attribution(v: &CoherentTrustReport) -> String {
    // Neutral `(wire basis code, count)` pairs — the agent breakdown never sees a trust
    // type (preserving `agent`'s no-dependency-on-`repo-graph-trust` boundary).
    let breakdown = attribution::attribution_breakdown(
        v.basis_classifications
            .value
            .iter()
            .map(|r| (r.basis_code.as_str(), r.count)),
    );
    if breakdown.is_empty() {
        return String::new();
    }

    // Heading carries the same (source, scope, freshness) honesty label as its Half-B siblings,
    // and NO internal vocabulary ("Unresolved Breakdown" / "Classification" are gone).
    let mut out = labelled_heading(
        "Unresolved references — where they go",
        &v.basis_classifications,
        "snapshot-scoped extraction",
    );
    for (class, count) in &breakdown.classes {
        if *class == attribution::AttributionClass::ExternalDependency {
            // The "library call" class is NAMED per DECLARED dependency (the provenance
            // join), NOT rendered as a bare class total.
            render_library_calls(&mut out, v);
        } else {
            out.push_str(&bullet(&attribution::attribution_line(*class, *count)));
        }
    }
    if breakdown.other > 0 {
        out.push_str(&bullet(&format!(
            "{}: {}",
            attribution::OTHER_UNRESOLVED_LABEL,
            attribution::count_references(breakdown.other)
        )));
    }
    // EY1-A honest basis (heuristic, not a Layer-0 edge claim) + the honest provenance
    // degradation (a named dependency is the declared dependency a reference resolved to — via
    // its specifier or its receiver/callee import; versions not recorded; Java/Gradle
    // heuristic — review-0 #3). The exact wording lives in `attribution::PROVENANCE_BASIS`.
    out.push_str(&bullet(attribution::ATTRIBUTION_BASIS));
    out.push_str(&bullet(attribution::PROVENANCE_BASIS));
    out
}

/// Render the "library call" ([`attribution::AttributionClass::ExternalDependency`]) class
/// as NAMED dependency lines (ATTRIBUTION-1 iteration 3): the top DECLARED dependencies by
/// name (from the `external_dependencies` provenance-join leaf), an honest aggregate tail for
/// identified-but-unlisted dependencies, and the honest "dependency not identified" bucket
/// for references that could not be resolved to a declared dependency. All three values come
/// from the ONE storage join, so `total_named + unidentified` reconciles the class total.
fn render_library_calls(out: &mut String, v: &CoherentTrustReport) {
    let attr = &v.external_dependencies.value;
    // The top declared dependencies (across all three external-import bases), already bounded
    // + count-desc/name-asc from the storage join. Each is the DECLARED manifest name.
    let mut shown = 0u64;
    for dep in &attr.top {
        out.push_str(&bullet(&attribution::named_dependency_line(
            &dep.name, dep.count,
        )));
        shown += dep.count;
    }
    // Identified dependencies beyond the bounded top-N list (honest aggregate tail — these
    // ARE named declared deps, just not individually listed; distinct from "not identified").
    let remainder = attr.total_named.saturating_sub(shown);
    if remainder > 0 {
        out.push_str(&bullet(&attribution::more_named_dependencies_line(
            remainder,
        )));
    }
    // External-import references with no nameable declared dependency — the honest
    // missing-name degradation (never a fabricated name).
    if attr.unidentified > 0 {
        out.push_str(&bullet(&attribution::dependency_not_identified_line(
            attr.unidentified,
        )));
    }
    // The orientation action (VISION), rendered once — only when there IS a named dependency
    // to follow.
    if attr.total_named > 0 {
        if let Some(hint) = attribution::AttributionClass::ExternalDependency.follow_hint() {
            out.push_str(&bullet(hint));
        }
    }

    // TRUST-FIRSTPARTY-1: references whose declared name is one of THIS repo's own packages
    // (workspace member / declared package — structural manifest facts, never a name prefix) are
    // NOT third-party libraries. Render them as internal crates with an IN-REPO next move, never
    // the crates.io/package-docs follow above (the exact defect this slice fixes). They are
    // already excluded from the external named lines + the external figure; together with
    // `total_named` + `unidentified` they reconcile the ExternalDependency class total.
    let mut fp_shown = 0u64;
    for dep in &attr.first_party {
        out.push_str(&bullet(&attribution::first_party_line(
            &dep.name, dep.count,
        )));
        fp_shown += dep.count;
    }
    // `checked_sub`, NOT `saturating_sub` (review-1 §2): the shown rows are a truncation of the
    // counted first-party set, so `fp_shown <= first_party_total` in any coherent report. If it does
    // NOT hold (a corrupt/foreign report), saturating to 0 would silently hide the reconciliation
    // error; instead render the remainder UNKNOWN with a reason (STANDING HONESTY RULE 1).
    match attr.first_party_total.checked_sub(fp_shown) {
        Some(fp_remainder) => {
            if fp_remainder > 0 {
                out.push_str(&bullet(&attribution::more_first_party_line(fp_remainder)));
            }
        }
        None => {
            out.push_str(&bullet(attribution::FIRST_PARTY_REMAINDER_UNRECONCILED));
        }
    }
    if attr.first_party_total > 0 {
        out.push_str(&bullet(attribution::FIRST_PARTY_FOLLOW));
    }
}

/// EY1-A (ENRICH-YIELD-2): a Layer-2 read projection over the enrichment metadata already on the
/// trust report — the largest reject class (~36% of enrichment-eligible unknown object-method calls)
/// whose receiver resolved to a *likely-external* type. This is ORIENTATION, not a resolved edge:
/// `<T>` tells the agent "this call goes into a library/std type `<T>` — follow it to that
/// crate/package/docs", converting an unattributed unknown into a place to look (VISION: orientation
/// over oracle). Read-only reprojection of `enrichment_status.top_types` (already computed by the
/// trust service); NO new persisted shape, NO new query — the ratified corrected EY1-A cell.
///
/// Two SEPARATE, independently-labelled basis lines (the ratified corrected EY1-A cell) — the two
/// facts have DISTINCT provenance and must not be conflated into one claim:
///   - receiver-type basis: the type name is inferred from a language-server type hover,
///     heuristically parsed;
///   - external-classification basis: the name matched a static name-set of well-known std/library
///     type names AND language primitives (EY1-B classifies primitives external), NOT
///     compiler-verified.
///
/// Both the Rust (`STD_TYPES` + `PRIMITIVES`) and TS (`NODE_TYPES`/`LIBRARY_TYPES`) resolvers
/// classify externality by static name-set, so the basis is accurate across languages; those
/// constant names are internal and kept off the reader surface (VISION: labels speak the reader's
/// language). Never claims Layer-0 certainty — the ratification rejected promoting these to edges.
fn render_enrichment_external(v: &CoherentTrustReport) -> String {
    // Enrichment never ran → honest absence (no section), never a measured-zero.
    let Some(status) = v.enrichment_status.value.as_ref() else {
        return String::new();
    };
    // review-3 §3: the top EXTERNAL targets are the service's `top_external_types` —
    // external-FILTERED then truncated — so a genuine top external is never dropped by the
    // mixed top-15 `top_types` cut. The zero-external "none identified" honesty rides the
    // Resolution external-share line (this section is the NAMED list; it only renders when
    // there are names to render).
    let external = &status.top_external_types;
    if external.is_empty() {
        return String::new();
    }
    let mut out = labelled_heading(
        "Likely-External Receiver Calls",
        &v.enrichment_status,
        "snapshot-scoped extraction",
    );
    // Two SEPARATE, independently-labelled bases (the ratified corrected EY1-A cell): the receiver
    // TYPE and the EXTERNAL classification are distinct heuristics with distinct provenance, so each
    // gets its own basis line — never merged into a single "basis:" claim. Plus the Layer-2 framing:
    // this is orientation, not a resolved edge. The basis strings + per-target line are the ONE shared
    // reader-frame vocabulary (`repo_graph_agent::reliability`) — the same `orient`'s compact coverage
    // map draws from, so the named map cannot fork across surfaces.
    out.push_str(&bullet(reliability::RECEIVER_TYPE_BASIS));
    out.push_str(&bullet(reliability::EXTERNAL_CLASSIFICATION_BASIS));
    out.push_str(&bullet(reliability::ORIENTATION_ONLY_BASIS));
    for t in external {
        let target = ExternalTarget {
            type_name: t.type_name.clone(),
            count: t.count,
        };
        out.push_str(&bullet(&CallReliabilityView::named_target_line(&target)));
    }
    out
}

fn render_suspicious_modules(v: &CoherentTrustReport) -> String {
    let suspicious: Vec<_> = v
        .modules
        .value
        .iter()
        .filter(|m| m.suspicious_zero_connectivity)
        .collect();
    if suspicious.is_empty() {
        return String::new();
    }
    let mut out = labelled_heading(
        "Suspicious Modules (zero connectivity)",
        &v.modules,
        "snapshot-scoped extraction",
    );
    // CONTRADICTION-SWEEP-1 §3: state the basis inline so this never reads as a
    // contradiction of `stats`. "Zero connectivity" here = zero RESOLVED
    // module-to-module import edges attributed to THIS module's directory node
    // (fan_in = fan_out = 0). A finer-grained CHILD module (e.g. a subdirectory)
    // can still carry connectivity that `stats` reports against its own node — so
    // the two surfaces are consistent at different granularity, not in conflict.
    // Computation is UNCHANGED (this slice aligns wording, not math).
    out.push_str(
        "  basis: no resolved module-to-module import edges attach to this module's \
         directory node (fan_in = fan_out = 0); a finer-grained child module may still \
         be connected — cross-check per-module fan_in/fan_out in `stats`.\n",
    );
    for m in suspicious.iter().take(10) {
        out.push_str(&bullet(&m.qualified_name));
    }
    if suspicious.len() > 10 {
        out.push_str(&bullet(&format!("... ({} more)", suspicious.len() - 10)));
    }
    out
}

fn render_downgrades(v: &CoherentTrustReport) -> String {
    let d = &v.triggered_downgrades.value;
    let mut items = Vec::new();

    if d.framework_heavy_suspicion.triggered {
        let reason = d
            .framework_heavy_suspicion
            .reasons
            .first()
            .map(|s| s.as_str())
            .unwrap_or("framework patterns detected");
        items.push(format!("framework_heavy_suspicion: {}", reason));
    }
    if d.registry_pattern_suspicion.triggered {
        let reason = d
            .registry_pattern_suspicion
            .reasons
            .first()
            .map(|s| s.as_str())
            .unwrap_or("registry patterns detected");
        items.push(format!("registry_pattern_suspicion: {}", reason));
    }
    if d.missing_entrypoint_declarations.triggered {
        let reason = d
            .missing_entrypoint_declarations
            .reasons
            .first()
            .map(|s| s.as_str())
            .unwrap_or("entrypoints not declared");
        items.push(format!("missing_entrypoint_declarations: {}", reason));
    }
    if d.alias_resolution_suspicion.triggered {
        let reason = d
            .alias_resolution_suspicion
            .reasons
            .first()
            .map(|s| s.as_str())
            .unwrap_or("alias resolution issues");
        items.push(format!("alias_resolution_suspicion: {}", reason));
    }

    if items.is_empty() {
        return String::new();
    }

    // The downgrade-triggers leaf is multi-source {sqlite, declaration} (D-TRUST-4): the label shows BOTH
    // contributing sources (the entrypoint Authority read).
    let mut out = labelled_heading(
        "Triggered Downgrades",
        &v.triggered_downgrades,
        "snapshot-scoped extraction",
    );
    for item in items {
        out.push_str(&bullet(&item));
    }
    out
}

fn format_axis(name: &str, axis: &ReliabilityAxisScore) -> String {
    format_axis_maybe_ceiling(name, axis, false)
}

/// Render one reliability axis. COHERENCE-POLISH-1 §2: when `at_ceiling`, the reason humanizer DROPS
/// the "(below N% target)" clause from the `call_resolution_rate` reason — on a permanent no-resolver
/// ceiling that target can never be approached, so naming it would imply an unimprovable number can
/// improve. The ceiling sentence (rendered by the caller) carries the WHY. `at_ceiling` is passed only
/// for the Call-graph axis (the only axis that carries a `call_resolution_rate` reason).
fn format_axis_maybe_ceiling(name: &str, axis: &ReliabilityAxisScore, at_ceiling: bool) -> String {
    let level = format!("{:?}", axis.level);
    if axis.reasons.is_empty() {
        format!("{}: {}", name, level)
    } else {
        // RELIABILITY-REFRAME-1: reader-frame reason prose from the ONE shared humanizer
        // (was a byte-for-byte copy that has now converged with orient's).
        let humanized: Vec<String> = axis
            .reasons
            .iter()
            .map(|r| {
                if at_ceiling {
                    reliability::humanize_reason_at_ceiling(r)
                } else {
                    reliability::humanize_reason(r)
                }
            })
            .collect();
        format!("{}: {} ({})", name, level, humanized.join("; "))
    }
}

fn truncate_uid(uid: &str) -> String {
    if uid.len() > 20 {
        format!("{}...", &uid[..17])
    } else {
        uid.to_string()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────────────────────────
//
// In a SIBLING file (`trust_tests.rs`) via `#[path]` to respect the >500-line structural guardrail
// (CLAUDE.md §Structural Guardrails). The tests build the wrapper through the real `trust_to_coherent`.
#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
