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
use crate::dto::signal::{Signal, SignalCode};

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

/// Truncate a signal list to the budget cap WITHOUT ever dropping a
/// load-bearing (`protected`) signal.
///
/// ORIENT-DENSITY-1 §2: "BUDGET CONTROLS DEPTH, NOT INFORMATION."
/// A budget is a density contract — it trades how much DEPTH the
/// presentation renders, never the PRESENCE of the load-bearing
/// orientation facts. The plain [`truncate_signals`] cap drops the
/// lowest-ranked tail, which at `small` can strip the structure
/// (`MODULE_SUMMARY`) / complexity / cycles signals the dense headline
/// is synthesized from — exactly the inversion the slice fixes.
///
/// This variant pins every signal whose code is in `protected`
/// (always kept, exempt from the cap) and applies the budget cap only
/// to the remaining tail. The retained protected signals stay in
/// rank order (the retain preserves order), so the JSON `rank`
/// sequence is unchanged. `truncated` / `omitted` describe ONLY the
/// unprotected tail that was actually dropped — a protected signal is
/// never counted as omitted. `Budget::Full` caps at `usize::MAX`, so
/// nothing truncates regardless of protection.
///
/// Determinism: `protected` membership is a pure function of the
/// signal code, and the surviving unprotected prefix is the existing
/// rank order — source-independent, identical run to run.
pub fn truncate_signals_protecting(
    signals: &mut Vec<Signal>,
    budget: Budget,
    protected: &[SignalCode],
) -> TruncationOutcome {
    let cap = budget.max_signals();
    let is_protected = |s: &Signal| protected.contains(&s.code());

    // Fast path: if the unprotected count already fits, nothing drops.
    let unprotected_total = signals.iter().filter(|s| !is_protected(s)).count();
    if unprotected_total <= cap {
        return TruncationOutcome {
            truncated: false,
            omitted: 0,
        };
    }

    // Keep every protected signal; keep the top `cap` unprotected (by the
    // existing rank order, which `retain` preserves); drop the rest.
    let mut kept_unprotected = 0usize;
    let mut omitted = 0usize;
    signals.retain(|s| {
        if is_protected(s) {
            true
        } else if kept_unprotected < cap {
            kept_unprotected += 1;
            true
        } else {
            omitted += 1;
            false
        }
    });

    TruncationOutcome {
        truncated: omitted > 0,
        omitted,
    }
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
                top_modules: Vec::new(),
                package_groups: Vec::new(),
                root_manifest_limitation: None,
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

    // ── ORIENT-DENSITY-1 §2: budget trades DEPTH, never the load-bearing facts ──

    /// Build 8 signals where MODULE_SUMMARY (informational, ranks LAST) would be
    /// the first to fall to a small cap — the exact "budget strips structure" bug.
    fn signals_with_low_ranked_module_summary() -> Vec<Signal> {
        let mut s = make_signals(); // 5 incl. MODULE_SUMMARY
                                    // Three more medium-rank trust signals so the unprotected count (7) exceeds
                                    // the small cap (5) and the lowest-ranked tail (MODULE_SUMMARY) is at risk.
        for rate in [0.41, 0.42, 0.43] {
            s.push(Signal::trust_low_resolution(TrustLowResolutionEvidence {
                resolution_rate: rate,
                resolved_count: (rate * 100.0) as u64,
                total_count: 100,
            }));
        }
        sort_and_rank(&mut s);
        s
    }

    #[test]
    fn truncate_protecting_pins_headline_signal_under_small_cap() {
        // First prove the bug the protection fixes: a PLAIN small cap drops the
        // low-ranked MODULE_SUMMARY (informational) — the "budget strips structure"
        // inversion ORIENT-DENSITY-1 targets.
        let mut plain = signals_with_low_ranked_module_summary();
        truncate_signals(&mut plain, Budget::Small);
        assert!(
            !plain.iter().any(|x| x.code() == SignalCode::ModuleSummary),
            "precondition: a plain cap DOES strip MODULE_SUMMARY at small"
        );

        // Now with protection it survives, and the cut bites the unprotected tail.
        let mut s = signals_with_low_ranked_module_summary();
        let outcome =
            truncate_signals_protecting(&mut s, Budget::Small, &[SignalCode::ModuleSummary]);
        assert!(
            s.iter().any(|x| x.code() == SignalCode::ModuleSummary),
            "MODULE_SUMMARY must survive a small budget (budget trades depth, not the headline)"
        );
        // 7 unprotected > cap 5 ⇒ 2 unprotected dropped; the protected one is extra.
        assert!(outcome.truncated);
        assert_eq!(outcome.omitted, 2);
    }

    #[test]
    fn truncate_protecting_no_drop_when_unprotected_fits() {
        // 5 signals, one protected ⇒ 4 unprotected ≤ small cap (5): nothing drops.
        let mut s = make_signals();
        sort_and_rank(&mut s);
        let outcome =
            truncate_signals_protecting(&mut s, Budget::Small, &[SignalCode::ModuleSummary]);
        assert!(!outcome.truncated);
        assert_eq!(outcome.omitted, 0);
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn truncate_protecting_full_budget_keeps_everything() {
        let mut s = signals_with_low_ranked_module_summary();
        let len = s.len();
        let outcome =
            truncate_signals_protecting(&mut s, Budget::Full, &[SignalCode::ModuleSummary]);
        assert!(!outcome.truncated, "--full never truncates");
        assert_eq!(outcome.omitted, 0);
        assert_eq!(s.len(), len);
    }

    #[test]
    fn truncate_protecting_empty_protected_matches_plain_cap() {
        // With no protected codes the behavior is the plain budget cap.
        let mut s = signals_with_low_ranked_module_summary();
        let outcome = truncate_signals_protecting(&mut s, Budget::Small, &[]);
        assert!(outcome.truncated);
        assert_eq!(s.len(), 5, "8 signals capped to small (5)");
        assert_eq!(outcome.omitted, 3);
    }
}
