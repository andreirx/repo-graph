//! Budget tier — hard caps on signals, limits, and next actions.
//!
//! Budget is a use-case input (how much detail the caller can
//! afford to render) and a ranking constraint (which signals
//! survive truncation). It is not the serialized output of any
//! command; it is consumed by the aggregator pipeline and
//! reflected in the output through `_truncated` / `_omitted_count`
//! fields on sections that were capped.
//!
//! Hard caps are locked at the per-tier methods below. Tuning the
//! caps is a single-site change.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Budget {
    #[default]
    Small,
    Medium,
    Large,
    /// Uncapped tier (TRUNCATION-AUDIT-1, the `--full` escape hatch).
    ///
    /// Every cap returns `usize::MAX`, so NO list truncates and every
    /// `*_truncated` flag is `false`. The meaningful pre-truncation
    /// ordering still applies (it is independent of the cap), so `--full`
    /// emits the COMPLETE list in the same deterministic order a capped
    /// tier would have used for its surviving prefix. Intended for
    /// `rmap <cmd> --full | grep <x>`.
    Full,
}

impl Budget {
    /// Maximum number of signals emitted at this budget tier.
    pub fn max_signals(self) -> usize {
        match self {
            Self::Small => 5,
            Self::Medium => 15,
            Self::Large => 50,
            Self::Full => usize::MAX,
        }
    }

    /// Maximum number of limit records emitted at this budget tier.
    pub fn max_limits(self) -> usize {
        match self {
            Self::Small => 3,
            Self::Medium => 5,
            Self::Large => 20,
            Self::Full => usize::MAX,
        }
    }

    /// Maximum number of next-action records emitted at this
    /// budget tier.
    pub fn max_next(self) -> usize {
        match self {
            Self::Small => 3,
            Self::Medium => 5,
            Self::Large => 10,
            Self::Full => usize::MAX,
        }
    }

    /// Maximum number of NAMED complexity centers carried in the
    /// `HIGH_COMPLEXITY` evidence (ORIENT-DENSITY-1 §5).
    ///
    /// This is the DEPTH knob the slice's review-1 #2 fix turns: before,
    /// the complexity evidence was hard-capped at 5 rows regardless of
    /// budget, so even `--full` rendered "(+338 more above threshold)".
    /// Now the EVIDENCE itself scales — `small` carries a lean headline
    /// set, `large`/`--full` carry EVERY above-threshold center so the
    /// `--full` breakdown is genuinely complete (§5: "all hotspots").
    /// `high_complexity_count` always reports the true total, so a capped
    /// tier never overclaims completeness.
    pub fn max_complexity_centers(self) -> usize {
        match self {
            Self::Small => 5,
            Self::Medium => 15,
            // large / --full = the complete detail (DoD: "large/`--full` =
            // complete"). The presentation still renders a DENSE top-N
            // headline line; the full SET rides the breakdown section.
            Self::Large | Self::Full => usize::MAX,
        }
    }

    /// Maximum number of NAMED modules carried in the `MODULE_SUMMARY`
    /// evidence (`top_modules`) for the dense structure headline
    /// (ORIENT-DENSITY-1 §5).
    ///
    /// Mirrors `max_complexity_centers`: `small`/`medium` carry a bounded
    /// headline set (each ≥ the presentation's display cap, so the named
    /// structure line is never under-fed), `large`/`--full` carry the FULL
    /// module list so the `--full` "Modules (by size)" breakdown is
    /// complete. `discovered_module_count` always reports the true total.
    pub fn max_modules(self) -> usize {
        match self {
            Self::Small => 12,
            Self::Medium => 24,
            Self::Large | Self::Full => usize::MAX,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_are_stable_per_tier() {
        assert_eq!(Budget::Small.max_signals(), 5);
        assert_eq!(Budget::Small.max_limits(), 3);
        assert_eq!(Budget::Small.max_next(), 3);
        assert_eq!(Budget::Medium.max_signals(), 15);
        assert_eq!(Budget::Medium.max_limits(), 5);
        assert_eq!(Budget::Medium.max_next(), 5);
        assert_eq!(Budget::Large.max_signals(), 50);
        assert_eq!(Budget::Large.max_limits(), 20);
        assert_eq!(Budget::Large.max_next(), 10);
    }

    #[test]
    fn full_is_uncapped() {
        // TRUNCATION-AUDIT-1: the `--full` tier uncaps every list so nothing truncates.
        assert_eq!(Budget::Full.max_signals(), usize::MAX);
        assert_eq!(Budget::Full.max_limits(), usize::MAX);
        assert_eq!(Budget::Full.max_next(), usize::MAX);
    }

    #[test]
    fn density_depth_caps_scale_with_budget() {
        // ORIENT-DENSITY-1 review-1 #2: the EVIDENCE depth (not just the rendered
        // display) scales with budget, so `--full` carries the complete set.
        assert_eq!(Budget::Small.max_complexity_centers(), 5);
        assert_eq!(Budget::Medium.max_complexity_centers(), 15);
        assert_eq!(Budget::Large.max_complexity_centers(), usize::MAX);
        assert_eq!(Budget::Full.max_complexity_centers(), usize::MAX);

        assert_eq!(Budget::Small.max_modules(), 12);
        assert_eq!(Budget::Medium.max_modules(), 24);
        assert_eq!(Budget::Large.max_modules(), usize::MAX);
        assert_eq!(Budget::Full.max_modules(), usize::MAX);
    }

    #[test]
    fn density_depth_caps_are_monotonic_and_feed_the_display() {
        // small ⊆ medium ⊆ large/full — the budget trades DEPTH, never inverts it.
        // And each tier's evidence cap is ≥ the presentation's display cap (modules:
        // 8/16, complexity: 3/5) so the named headline is never under-fed.
        assert!(Budget::Small.max_complexity_centers() <= Budget::Medium.max_complexity_centers());
        assert!(Budget::Medium.max_complexity_centers() <= Budget::Large.max_complexity_centers());
        assert!(Budget::Small.max_modules() <= Budget::Medium.max_modules());
        assert!(Budget::Medium.max_modules() <= Budget::Large.max_modules());
        assert!(
            Budget::Small.max_modules() >= 8,
            "feeds the 8-name small display"
        );
        assert!(
            Budget::Small.max_complexity_centers() >= 3,
            "feeds the 3-center small display"
        );
    }

    #[test]
    fn full_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Budget::Full).unwrap(), "\"full\"");
    }

    #[test]
    fn default_is_small() {
        assert_eq!(Budget::default(), Budget::Small);
    }

    #[test]
    fn serializes_lowercase() {
        let s = serde_json::to_string(&Budget::Small).unwrap();
        assert_eq!(s, "\"small\"");
    }
}
