//! FIXTURE-POLLUTION-1 §2.2/§2.3 — the presentation-side decode of a cycle's
//! test-composition discriminant.
//!
//! Split out of the sibling [`super`] renderer so the DTO/rendering file holds the 500-line
//! guardrail (review-1 finding #3). This is a thin, total decode of the daemon's per-cycle
//! `test_composition` string into the renderer's four-state view — the actual classification
//! (the stored `is_test` fact, conservatively aggregated) is done DAEMON-side in
//! `cycle_output::composition`; here we only interpret its result.
//!
//! Abstraction record — module: `presentation::cycles::composition`; concrete current user:
//! [`super::CyclesResponse::render_human`]; axis: the §2.2 test-only/unknown/not-evaluated
//! partition of the rendered cycles, kept off the renderer file to hold the guardrail;
//! rejected simpler alternative: leaving the enum + decoder inline (part of the
//! over-guardrail state review-1 flagged). Accesses the parent's private [`super::Cycle`]
//! directly (descendant visibility) — no wider `pub` leak.

use super::Cycle;

/// FIXTURE-POLLUTION-1 binding direction rule: a cycle's test-composition as seen by the
/// renderer. `NotEvaluated` (the field is absent) is the LiveGraph route — rendered in the
/// main listing with NO per-cycle marker, since the response-level asymmetry note covers it.
/// Only `TestOnly` demotes; `Unknown` stays in the main listing WITH a marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CycleComposition {
    TestOnly,
    Production,
    Unknown(String),
    NotEvaluated,
}

impl Cycle {
    /// Decode the daemon discriminant. An ABSENT field is `NotEvaluated` (LiveGraph route);
    /// an `unknown` discriminant carries its reason (falling back to a fixed explanation only
    /// if the daemon omitted it — a display string, not a classification); any unrecognized
    /// value is treated as `Unknown` rather than silently as production.
    pub(super) fn composition(&self) -> CycleComposition {
        match self.test_composition.as_deref() {
            None => CycleComposition::NotEvaluated,
            Some("test_only") => CycleComposition::TestOnly,
            Some("production") => CycleComposition::Production,
            Some(_) => CycleComposition::Unknown(match &self.test_composition_unknown_reason {
                Some(reason) => reason.clone(),
                None => "test-composition could not be evaluated".to_string(),
            }),
        }
    }
}
