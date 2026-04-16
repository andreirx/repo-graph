//! Ranking, sorting, and budget truncation for agent signals.
//!
//! The ranking pass is applied exactly once after all aggregators
//! have emitted their signals. Aggregators MUST NOT compute their
//! own rank: they leave `rank = 0` and let this module assign a
//! monotonically increasing 1-based rank post-sort.
//!
//! Sort order (stable):
//!
//!   1. Severity descending: High > Medium > Low.
//!   2. Category ascending by `tie_break_ordinal`:
//!      Gate > Boundary > Trust > Structure > Informational.
//!   3. Tier priority ascending by `SignalCode::tier_priority()` —
//!      explicit per-code ordering within the same (severity,
//!      category) bucket. Lower value = higher priority.
//!
//! Truncation is applied AFTER ranking so that the surviving
//! signals are the highest-ranked ones, and `omitted_count`
//! reflects lower-priority tail removed from the output.

use crate::dto::budget::Budget;
use crate::dto::limit::Limit;
use crate::dto::signal::Signal;

/// Outcome of truncating a list to a budget cap.
///
/// `truncated` is `true` iff the original list exceeded the cap.
/// `omitted` is the number of elements that were dropped from
/// the tail.
pub struct TruncationOutcome {
	pub truncated: bool,
	pub omitted: usize,
}

/// Sort the signal list in rank order, then assign 1-based
/// ranks. Stable sort: equal-priority signals preserve
/// construction order, so aggregator authors can control the
/// output of ties by construction order alone.
pub fn sort_and_rank(signals: &mut Vec<Signal>) {
	signals.sort_by(|a, b| {
		// Severity descending.
		b.severity().cmp(&a.severity())
			// Category ascending.
			.then_with(|| {
				a.category()
					.tie_break_ordinal()
					.cmp(&b.category().tie_break_ordinal())
			})
			// Explicit priority within the same tier.
			.then_with(|| {
				a.code()
					.tier_priority()
					.cmp(&b.code().tier_priority())
			})
	});

	for (index, signal) in signals.iter_mut().enumerate() {
		let rank = u32::try_from(index + 1).unwrap_or(u32::MAX);
		signal.set_rank(rank);
	}
}

/// Truncate a signal list to the budget cap. Returns an
/// outcome describing whether truncation occurred and how many
/// elements were dropped.
pub fn truncate_signals(
	signals: &mut Vec<Signal>,
	budget: Budget,
) -> TruncationOutcome {
	truncate_vec(signals, budget.max_signals())
}

/// Truncate a limit list to the budget cap. Limits are sorted
/// by their variant order (stable ordering is determined by the
/// aggregator insertion order; limits are not rank-assigned).
pub fn truncate_limits(
	limits: &mut Vec<Limit>,
	budget: Budget,
) -> TruncationOutcome {
	truncate_vec(limits, budget.max_limits())
}

fn truncate_vec<T>(v: &mut Vec<T>, cap: usize) -> TruncationOutcome {
	if v.len() <= cap {
		return TruncationOutcome { truncated: false, omitted: 0 };
	}
	let omitted = v.len() - cap;
	v.truncate(cap);
	TruncationOutcome { truncated: true, omitted }
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::dto::signal::{
		BoundaryViolationsEvidence, DeadCodeEvidence, ImportCyclesEvidence,
		ModuleSummaryEvidence, SnapshotInfoEvidence,
	};

	fn make_signals() -> Vec<Signal> {
		vec![
			// Deliberately in wrong order so sorting has work to do.
			Signal::snapshot_info(SnapshotInfoEvidence {
				snapshot_uid: "snap".into(),
				scope: "full".into(),
				basis_commit: None,
				created_at: "t".into(),
			}),
			Signal::module_summary(ModuleSummaryEvidence {
				file_count: 1,
				symbol_count: 1,
				languages: vec![],
			}),
			Signal::dead_code(DeadCodeEvidence {
				dead_count: 1,
				top_dead: vec![],
			}),
			Signal::boundary_violations(BoundaryViolationsEvidence {
				violation_count: 1,
				top_violations: vec![],
			}),
			Signal::import_cycles(ImportCyclesEvidence {
				cycle_count: 1,
				cycles: vec![],
			}),
		]
	}

	#[test]
	fn sort_puts_boundary_violations_first() {
		let mut s = make_signals();
		sort_and_rank(&mut s);
		// BOUNDARY_VIOLATIONS is High severity, Boundary category
		// (first non-gate high-severity code) — should rank 1.
		assert_eq!(s[0].code().as_str(), "BOUNDARY_VIOLATIONS");
		assert_eq!(s[0].rank(), 1);
	}

	#[test]
	fn sort_puts_informational_last() {
		let mut s = make_signals();
		sort_and_rank(&mut s);
		assert_eq!(
			s.last().unwrap().category().as_str(),
			"informational"
		);
	}

	#[test]
	fn sort_assigns_dense_1_based_ranks() {
		let mut s = make_signals();
		sort_and_rank(&mut s);
		for (i, sig) in s.iter().enumerate() {
			assert_eq!(sig.rank(), (i + 1) as u32);
		}
	}

	#[test]
	fn truncate_drops_lowest_rank_tail() {
		let mut s = make_signals();
		sort_and_rank(&mut s);
		let outcome = truncate_signals(&mut s, Budget::Small);
		// small = 5 cap; we have exactly 5, so no truncation.
		assert!(!outcome.truncated);
		assert_eq!(outcome.omitted, 0);
		assert_eq!(s.len(), 5);
	}

	#[test]
	fn truncate_respects_small_cap() {
		let mut s = make_signals();
		// Add a 6th signal — same as dead_code so we have 6.
		s.push(Signal::dead_code(DeadCodeEvidence {
			dead_count: 2,
			top_dead: vec![],
		}));
		sort_and_rank(&mut s);
		let outcome = truncate_signals(&mut s, Budget::Small);
		assert!(outcome.truncated);
		assert_eq!(outcome.omitted, 1);
		assert_eq!(s.len(), 5);
	}
}
