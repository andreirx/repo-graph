//! Reliability-caveat renderers for the `orient` command headline.
//!
//! Split out of `orient_sections.rs` to keep each module under the 500-line
//! structural guardrail (review-1 §3) — the `orient.rs` / `orient_sections.rs` /
//! `orient_reliability.rs` split idiom, extended. A continuation `impl OrientResponse`
//! block (inherent impls may span modules within the crate). Pure relocation — no
//! behavior changed. It owns the compressed reader-frame reliability caveat and the
//! shared `CallReliabilityView` projection the `--full` Degradation / External-coverage
//! sections (in `orient_reliability.rs`) also consume, so the one-liner and the full
//! surface cannot fork.

use repo_graph_agent::reliability::{self, CallReliabilityView, ExternalTarget};

use super::orient::{OrientResponse, ReliabilityAxis};

impl OrientResponse {
    /// The compressed RELIABILITY caveat (ORIENT-DENSITY-1 §3.5) — the reader-frame in-scope
    /// rate + band + "verify against source", NOT the three-axis Degradation block. Rendered at
    /// EVERY budget, so per iteration-5 §2 it honors the ratified unknown/unclassified rules: a
    /// zero-in-scope (all-external) repo reads "no in-scope calls measured" + compact external
    /// context (never silence), and a material unclassified share appends a compact second caveat
    /// line. `None` when trust is high with nothing degraded, or there is no briefing. The full
    /// per-axis breakdown + named coverage map still render at `--full` (`render_degradation` /
    /// `render_external_coverage`).
    pub(super) fn reliability_caveat_line(&self) -> Option<String> {
        let trust = self.trust_briefing.as_ref()?;

        if let Some(rel) = &trust.reliability {
            if let Some(cg) = &rel.call_graph {
                // RELIABILITY-REFRAME-1 (iteration-5 §2): the ratified unknown/unclassified rules
                // apply at EVERY budget, so this compressed headline honors them from the SAME
                // shared projection the `--full` surface uses (built once here, from the overlay's
                // real call-coverage COUNTS — not a rate parsed out of prose).
                let view = self.call_reliability_view(Some(&cg.level));

                // Zero in-scope calls (an all-external repo → the vacuous 0-of-0 HIGH band):
                // `resolution == None` distinguishes it from a genuine HIGH. It must NOT fall
                // silent — render the honest "no in-scope calls measured" (unknown, never a
                // fabricated 100%) + the external share as compact context, at every budget.
                if let Some(v) = &view {
                    // review-6 §1: an EMPTY call graph (total_calls == 0) is still a
                    // zero-in-scope measurement — the vacuous HIGH band must not fall
                    // silent either. No `total_calls > 0` gate.
                    if v.resolution.is_none() {
                        return Some(self.zero_in_scope_caveat_line(v));
                    }
                }

                if cg.level != "HIGH" {
                    // The reader-frame in-scope rate + band. Falls back to the rate-only path when
                    // the overlay predates `call_coverage`; both go through the same reader-frame
                    // wording, so the one-liner and the full Degradation agree and neither grades
                    // repo-graph.
                    let detail = if let Some(v) = &view {
                        v.resolved_with_band()
                    } else if let Some(pct) = self.call_resolution_pct(cg) {
                        reliability::resolved_phrase_with_band(pct, &cg.level)
                    } else {
                        format!("your code's call resolution is {}", cg.level)
                    };
                    let mut line = format!(
                        "Reliability: {} — verify call/dead claims against source.",
                        detail
                    );
                    // The material-unclassified qualification (review-3 §2) rides the headline as a
                    // compact second line — from the SAME shared helper the `--full` External calls
                    // section uses, so the rate's honest lower-bound caveat cannot fork or vanish at
                    // the default budget.
                    if let Some(caveat) = self.material_unclassified_caveat(view.as_ref()) {
                        line.push('\n');
                        line.push_str(&caveat);
                    }
                    return Some(line);
                }
            }
            // call-graph is fine; surface the worst remaining degraded axis.
            for (name, axis) in [
                ("import-graph", &rel.import_graph),
                ("change-impact", &rel.change_impact),
            ] {
                if let Some(ax) = axis {
                    if ax.level != "HIGH" {
                        return Some(format!(
                            "Reliability: {} reliability {} — verify against source.",
                            name, ax.level
                        ));
                    }
                }
            }
            return None;
        }

        // Legacy TrustOverlay fields (backward compatibility). Same in-scope
        // `call_resolution_rate` value, reframed to the reader's terms.
        if let Some(rate) = trust.call_resolution_rate {
            if rate < 0.95 {
                return Some(format!(
                    "Reliability: {} — verify call/dead claims against source.",
                    reliability::resolved_phrase_pct(rate * 100.0)
                ));
            }
        }
        if let Some(level) = &trust.call_graph_reliability {
            if level != "high" {
                return Some(format!(
                    "Reliability: your code's call resolution is {} — verify call/dead claims against source.",
                    level
                ));
            }
        }
        None
    }

    /// Extract the raw call-resolution percentage from a reliability axis's
    /// machine reasons (`call_resolution_rate=42.0%_below_50%`). Returned
    /// unrounded so the caller can format it identically to `humanize_reason`
    /// (`{:.0}`), keeping the headline and the full Degradation consistent.
    pub(super) fn call_resolution_pct(&self, axis: &ReliabilityAxis) -> Option<f64> {
        for r in &axis.reasons {
            if let Some(rest) = r.strip_prefix("call_resolution_rate=") {
                let num = rest.split('%').next().unwrap_or("");
                if let Ok(v) = num.parse::<f64>() {
                    return Some(v);
                }
            }
        }
        None
    }

    /// Build the ONE shared reader-frame projection
    /// ([`repo_graph_agent::reliability::CallReliabilityView`]) from the trust overlay's
    /// call-coverage COUNTS — the same derivation `trust` and `check` consume, so orient's
    /// in-scope rate / external share / named coverage map cannot diverge from theirs.
    /// `band` is the serialized call-graph level ("LOW"/…), mapped to the typed band.
    /// `None` when the overlay predates `call_coverage` (older daemon) — callers fall back
    /// to the rate-only legacy path.
    pub(super) fn call_reliability_view(&self, band: Option<&str>) -> Option<CallReliabilityView> {
        let cov = self.trust_briefing.as_ref()?.call_coverage.as_ref()?;
        let total_calls = cov.resolved_calls + cov.unresolved_calls;
        // Already external-filtered + count-desc at the producer; re-filter defensively so a
        // non-external target can never leak into the reader's coverage map.
        let named: Vec<ExternalTarget> = cov
            .external_targets
            .iter()
            .filter(|t| t.is_external && t.count > 0)
            .map(|t| ExternalTarget {
                type_name: t.type_name.clone(),
                count: t.count,
            })
            .collect();
        Some(CallReliabilityView::derive(
            cov.resolved_calls,
            cov.unresolved_calls_internal_like,
            cov.unresolved_calls_external,
            total_calls,
            named,
            band.and_then(reliability::band_from_wire),
        ))
    }

    /// The compressed headline for a repo with NO in-scope calls to grade — an all-external
    /// repo, which yields the vacuous 0-of-0 HIGH band. Renders the honest
    /// "no in-scope calls measured" (unknown, never silence, never a fabricated 100%) followed
    /// by the external share as compact context, so even the small-budget reader learns WHERE
    /// the calls go. Both strings come from the ONE shared projection; the fuller NAMED map is
    /// the `--full` External calls section. `view.external_line()` is guaranteed `Some` here —
    /// zero in-scope with calls present means every call is external.
    fn zero_in_scope_caveat_line(&self, view: &CallReliabilityView) -> String {
        let mut line = format!("Reliability: {}", reliability::NO_IN_SCOPE_CALLS);
        if let Some(external) = view.external_line() {
            line.push_str("; ");
            line.push_str(&external);
        }
        line.push('.');
        line
    }

    /// The conservative-rate caveat (review-3 §2) shared by the compressed headline and the
    /// `--full` External calls section — ONE home, so the two surfaces cannot fork. `None` when
    /// there is no in-scope rate, no `call_coverage` counts, or the unclassified share is
    /// immaterial ([`reliability::unclassified_caveat`] is the single materiality gate). The
    /// unclassified count and in-scope denominator both come from the SAME overlay the view is
    /// built from, so the caveat can never contradict the rate it qualifies.
    pub(super) fn material_unclassified_caveat(
        &self,
        view: Option<&CallReliabilityView>,
    ) -> Option<String> {
        let res = view?.resolution?;
        let cov = self.trust_briefing.as_ref()?.call_coverage.as_ref()?;
        reliability::unclassified_caveat(
            cov.unresolved_calls_unknown,
            res.in_scope_or_unclassified_total,
        )
    }
}
