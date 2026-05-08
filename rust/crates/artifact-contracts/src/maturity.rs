//! Classification maturity enumeration.
//!
//! Allows honest classification of families whose semantics
//! are still being determined.

use serde::{Deserialize, Serialize};

/// Maturity level of an artifact family's classification.
///
/// Not all families have fully stable semantics. This enum
/// allows the registry to be honest about which classifications
/// are provisional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationMaturity {
    /// Classification is stable and unlikely to change.
    ///
    /// The family's truth kind, refresh policy, and other
    /// contracts are well understood and proven in practice.
    Stable,

    /// Classification is provisional and may be refined.
    ///
    /// The family exists and is used, but its exact semantics
    /// are still being determined through real usage.
    Provisional,

    /// Classification is experimental and may change significantly.
    ///
    /// The family is new or being reworked. Classifications
    /// should be treated as best-effort guidance.
    Experimental,
}

impl ClassificationMaturity {
    /// Returns true if this classification is stable.
    pub fn is_stable(&self) -> bool {
        matches!(self, Self::Stable)
    }

    /// Returns true if this classification may change.
    pub fn may_change(&self) -> bool {
        matches!(self, Self::Provisional | Self::Experimental)
    }
}

impl std::fmt::Display for ClassificationMaturity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stable => write!(f, "stable"),
            Self::Provisional => write!(f, "provisional"),
            Self::Experimental => write!(f, "experimental"),
        }
    }
}
