//! DTO modules for the agent orientation surface.
//!
//! Every type the JSON contract mentions is declared under this
//! module tree. Nothing here depends on storage, trust,
//! indexer, or any adapter crate. These types are pure data.

pub mod budget;
pub mod envelope;
pub mod limit;
pub mod signal;
pub mod source;

pub use budget::Budget;
pub use envelope::{
	Confidence, Focus, FocusCandidate, FocusFailureReason, NextAction, NextKind,
	OrientResult, ResolvedKind, ORIENT_COMMAND, ORIENT_SCHEMA,
};
pub use limit::{Limit, LimitCode};
pub use signal::{
	BoundaryViolationEvidence, BoundaryViolationsEvidence, CycleEvidence,
	DeadCodeEvidence, DeadSymbolEvidence, ImportCyclesEvidence,
	ModuleSummaryEvidence, Severity, Signal, SignalCategory, SignalCode,
	SignalEvidence, SnapshotInfoEvidence, TrustLowResolutionEvidence,
	TrustNoEnrichmentEvidence, TrustStaleSnapshotEvidence,
};
pub use source::SourceRef;
