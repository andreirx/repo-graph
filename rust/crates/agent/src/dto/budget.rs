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
