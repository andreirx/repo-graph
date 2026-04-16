//! Confidence derivation for the orient envelope.
//!
//! Confidence is a single field (`high` / `medium` / `low`) that
//! tells the agent how much to trust the rest of the response.
//! It is derived from raw trust data, not from the emitted
//! signals: lossy signal truncation must not feed back into
//! confidence.
//!
//! ── Rules (Rust-43 F2 fix) ───────────────────────────────────
//!
//! Inputs:
//!   - call_resolution_rate: float in [0.0, 1.0]
//!   - stale: bool — `true` iff `get_stale_files` returned a
//!     non-empty list
//!   - enrichment_state: `EnrichmentState` three-state enum
//!
//! Output tiers:
//!
//!   - low     — resolution rate < 0.20
//!   - medium  — resolution rate in [0.20, 0.50]
//!               OR rate > 0.50 AND stale
//!               OR rate > 0.50 AND enrichment_state is NotRun
//!   - high    — rate > 0.50 AND not stale AND enrichment
//!               state is Ran or NotApplicable
//!
//! The enrichment axis penalizes only `NotRun`. `Ran` and
//! `NotApplicable` both indicate that enrichment is not a
//! concern on this axis (either the phase completed, or there
//! was nothing for it to do). The previous Rust-42 rule
//! `!applied && eligible > 0` collapsed three distinct states
//! into two and caused the F2 bug — see
//! `docs/spikes/2026-04-15-orient-on-repo-graph.md`.
//!
//! Order of checks: low beats medium beats high. Short-circuit
//! from the worst case upward.
//!
//! NOTE: focus-scoped "unresolved pressure" mentioned in the
//! contract is a module/symbol-focus concern and does not apply
//! to repo-level orient in the current scope.

use crate::dto::envelope::Confidence;
use crate::storage_port::{AgentTrustSummary, EnrichmentState};

pub fn derive_repo_confidence(
	trust: &AgentTrustSummary,
	stale: bool,
) -> Confidence {
	let rate = trust.call_resolution_rate;

	if rate < 0.20 {
		return Confidence::Low;
	}

	if rate <= 0.50 {
		return Confidence::Medium;
	}

	// Rate > 0.50 — potentially high, but degrade on stale or
	// unknown enrichment state.
	if stale {
		return Confidence::Medium;
	}

	match trust.enrichment_state {
		// Phase never ran. The agent cannot be confident that
		// the call graph was ever enriched — degrade.
		EnrichmentState::NotRun => Confidence::Medium,
		// Phase executed (regardless of success count). No
		// penalty on the enrichment axis.
		EnrichmentState::Ran => Confidence::High,
		// Phase executed with nothing to do. No penalty.
		EnrichmentState::NotApplicable => Confidence::High,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::storage_port::{AgentReliabilityAxis, AgentReliabilityLevel};

	fn reliable_axis() -> AgentReliabilityAxis {
		AgentReliabilityAxis {
			level: AgentReliabilityLevel::High,
			reasons: Vec::new(),
		}
	}

	fn ts(
		rate: f64,
		enrichment_state: EnrichmentState,
		eligible: u64,
	) -> AgentTrustSummary {
		AgentTrustSummary {
			call_resolution_rate: rate,
			resolved_calls: 0,
			unresolved_calls: 0,
			call_graph_reliability: reliable_axis(),
			dead_code_reliability: reliable_axis(),
			enrichment_state,
			enrichment_eligible: eligible,
			enrichment_enriched: match enrichment_state {
				EnrichmentState::Ran => 1,
				_ => 0,
			},
		}
	}

	#[test]
	fn low_below_20_percent() {
		assert_eq!(
			derive_repo_confidence(&ts(0.10, EnrichmentState::Ran, 10), false),
			Confidence::Low
		);
		assert_eq!(
			derive_repo_confidence(&ts(0.19, EnrichmentState::Ran, 10), false),
			Confidence::Low
		);
	}

	#[test]
	fn medium_in_20_to_50_percent_band() {
		assert_eq!(
			derive_repo_confidence(&ts(0.20, EnrichmentState::Ran, 10), false),
			Confidence::Medium
		);
		assert_eq!(
			derive_repo_confidence(&ts(0.50, EnrichmentState::Ran, 10), false),
			Confidence::Medium
		);
	}

	#[test]
	fn high_above_50_percent_clean_with_ran_enrichment() {
		assert_eq!(
			derive_repo_confidence(&ts(0.55, EnrichmentState::Ran, 10), false),
			Confidence::High
		);
		assert_eq!(
			derive_repo_confidence(&ts(0.99, EnrichmentState::Ran, 10), false),
			Confidence::High
		);
	}

	#[test]
	fn high_rate_but_stale_degrades_to_medium() {
		assert_eq!(
			derive_repo_confidence(&ts(0.80, EnrichmentState::Ran, 10), true),
			Confidence::Medium
		);
	}

	#[test]
	fn high_rate_but_enrichment_not_run_degrades_to_medium() {
		// Rust-43 F2 regression: previously called
		// "high_rate_but_no_enrichment_degrades_to_medium" with
		// `applied=false, eligible=10`. That combination is
		// unreachable in the new model (the storage adapter
		// maps `eligible > 0` to `Ran`, not `NotRun`). The
		// meaningful degrade case is NotRun — phase never
		// executed — which is what this test pins.
		assert_eq!(
			derive_repo_confidence(&ts(0.80, EnrichmentState::NotRun, 0), false),
			Confidence::Medium
		);
	}

	#[test]
	fn high_rate_with_not_applicable_enrichment_stays_high() {
		// Replaces "high_rate_with_zero_enrichment_eligible_stays_high".
		// Semantically identical case in the new model: the
		// storage adapter maps `Some(es) where es.eligible == 0`
		// to `NotApplicable`.
		assert_eq!(
			derive_repo_confidence(
				&ts(0.80, EnrichmentState::NotApplicable, 0),
				false
			),
			Confidence::High
		);
	}
}
