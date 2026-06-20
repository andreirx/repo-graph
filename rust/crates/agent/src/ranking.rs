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
pub fn sort_and_rank(signals: &mut [Signal]) {
    signals.sort_by(|a, b| {
        // Severity descending.
        b.severity()
            .cmp(&a.severity())
            // Category ascending.
            .then_with(|| {
                a.category()
                    .tie_break_ordinal()
                    .cmp(&b.category().tie_break_ordinal())
            })
            // Explicit priority within the same tier.
            .then_with(|| a.code().tier_priority().cmp(&b.code().tier_priority()))
    });

    for (index, signal) in signals.iter_mut().enumerate() {
        let rank = u32::try_from(index + 1).unwrap_or(u32::MAX);
        signal.set_rank(rank);
    }
}

/// Truncate a signal list to the budget cap. Returns an
/// outcome describing whether truncation occurred and how many
/// elements were dropped.
pub fn truncate_signals(signals: &mut Vec<Signal>, budget: Budget) -> TruncationOutcome {
    truncate_vec(signals, budget.max_signals())
}

/// Truncate a limit list to the budget cap.
///
/// TRUNCATION-AUDIT-1 audit (`truncate_limits`): limits are NOT re-sorted before the cut, and
/// — unlike signals — deliberately so. Signals carry a relevance rank (`sort_and_rank`:
/// severity/category/tier); limits do NOT. A limit is an orthogonal capability-gap marker
/// ("gate not configured", "complexity unavailable", "module data unavailable", …) with no
/// severity to rank by. Re-sorting them is unnecessary AND inventing a relevance order would be
/// a false-certainty claim the VISION forbids.
///
/// The pre-cut order is already deterministic, total, and SOURCE-INDEPENDENT — the property the
/// DR-EXPLAIN-CALLER-ORDER fix protects:
///   - DETERMINISTIC + TOTAL: every limit is a distinct `Limit::from_code(..)` pushed in a fixed
///     code-path order (the aggregator pipeline order — structural → governance → capability).
///     Distinct codes in fixed positions ⇒ no ties to break.
///   - SOURCE-INDEPENDENT: each limit is derived from a boolean/count CONDITION on repo state
///     (does a requirement exist? are complexity measurements present?), NEVER projected from a
///     storage-row iteration. So the list is a pure function of repo STATE — identical whether
///     SQLite or the LiveGraph answered. (Contrast the item lists in `ordering.rs`, which ARE
///     read from storage rows and therefore DO need an explicit total sort.)
///
/// Truncation drops the trailing markers; the cut is reported by `limits_truncated` /
/// `limits_omitted_count`, and `--full` (`Budget::Full`) uncaps it. NOTE: the coherence layer's
/// envelope-level provenance limits are appended AFTER this cut (`coherent::append_provenance_limits`,
/// itself deterministic), so they are never truncated here.
pub fn truncate_limits(limits: &mut Vec<Limit>, budget: Budget) -> TruncationOutcome {
    truncate_vec(limits, budget.max_limits())
}

fn truncate_vec<T>(v: &mut Vec<T>, cap: usize) -> TruncationOutcome {
    if v.len() <= cap {
        return TruncationOutcome {
            truncated: false,
            omitted: 0,
        };
    }
    let omitted = v.len() - cap;
    v.truncate(cap);
    TruncationOutcome {
        truncated: true,
        omitted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::signal::{
        BoundaryViolationsEvidence, ImportCyclesEvidence, ModuleSummaryEvidence,
        SnapshotInfoEvidence, TrustLowResolutionEvidence,
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
                discovered_module_count: None,
                module_kinds: None,
            }),
            Signal::trust_low_resolution(TrustLowResolutionEvidence {
                resolution_rate: 0.5,
                resolved_count: 50,
                total_count: 100,
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
        assert_eq!(s.last().unwrap().category().as_str(), "informational");
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
        // Add a 6th signal so we have 6.
        s.push(Signal::trust_low_resolution(TrustLowResolutionEvidence {
            resolution_rate: 0.4,
            resolved_count: 40,
            total_count: 100,
        }));
        sort_and_rank(&mut s);
        let outcome = truncate_signals(&mut s, Budget::Small);
        assert!(outcome.truncated);
        assert_eq!(outcome.omitted, 1);
        assert_eq!(s.len(), 5);
    }

    // ── TRUNCATION-AUDIT-1: limit truncation is deterministic ────────────
    //
    // Limits carry no relevance rank (audit decision: see `truncate_limits` doc). The cut keeps the
    // leading construction-order prefix verbatim — no re-sort — and flags the omission. This pins
    // that the cut is a deterministic prefix of the (source-independent) construction order.

    #[test]
    fn truncate_limits_keeps_construction_order_prefix() {
        use crate::dto::limit::{Limit, LimitCode};
        // A fixed-order limit list as the aggregator pipeline builds it, longer than the Small cap (3).
        let mut limits = vec![
            Limit::from_code(LimitCode::GateNotConfigured),
            Limit::from_code(LimitCode::ComplexityUnavailable),
            Limit::from_code(LimitCode::ModuleDataUnavailable),
            Limit::from_code(LimitCode::LanguageCoveragePartial),
        ];
        let original: Vec<LimitCode> = limits.iter().map(|l| l.code).collect();
        let outcome = truncate_limits(&mut limits, Budget::Small);
        assert!(outcome.truncated, "4 limits > Small cap (3) ⇒ truncated");
        assert_eq!(outcome.omitted, 1);
        let kept: Vec<LimitCode> = limits.iter().map(|l| l.code).collect();
        assert_eq!(
            kept,
            original[..3].to_vec(),
            "truncate_limits keeps the leading construction-order prefix verbatim (no re-sort)"
        );
    }

    #[test]
    fn truncate_limits_full_budget_keeps_all() {
        use crate::dto::limit::{Limit, LimitCode};
        let mut limits = vec![
            Limit::from_code(LimitCode::GateNotConfigured),
            Limit::from_code(LimitCode::ComplexityUnavailable),
            Limit::from_code(LimitCode::ModuleDataUnavailable),
            Limit::from_code(LimitCode::LanguageCoveragePartial),
        ];
        let outcome = truncate_limits(&mut limits, Budget::Full);
        assert!(!outcome.truncated, "--full uncaps the limit list");
        assert_eq!(outcome.omitted, 0);
        assert_eq!(limits.len(), 4);
    }
}
