//! Repo-level structural summary.
//!
//! Emits `MODULE_SUMMARY` unconditionally with snapshot-level
//! totals: file count, symbol count, language list.
//!
//! Rust-42 deliberately uses raw snapshot counts here, NOT
//! discovered module totals from the (still TS-only) module
//! discovery layer. The aggregator pairs the signal with a
//! `MODULE_DATA_UNAVAILABLE` limit so the agent can tell the
//! difference between "no discovered modules" and "module
//! discovery data is not queryable from Rust".
//!
//! The signal itself is NEVER suppressed by the limit — they
//! are orthogonal. The signal says "here is what the snapshot
//! contains", the limit says "there is a richer module catalog
//! you cannot see from this path".

use super::AggregatorOutput;
use crate::dto::limit::{Limit, LimitCode};
use crate::dto::signal::{ModuleSummaryEvidence, Signal};
use crate::errors::AgentStorageError;
use crate::storage_port::AgentStorageRead;

pub fn aggregate<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
	let summary = storage.compute_repo_summary(snapshot_uid)?;

	let evidence = ModuleSummaryEvidence {
		file_count: summary.file_count,
		symbol_count: summary.symbol_count,
		languages: summary.languages,
	};

	// LANGUAGE_COVERAGE_PARTIAL is defined as a LimitCode but
	// NOT emitted here. The Rust-43 F5 review identified that
	// emitting it unconditionally overclaims: a pure TypeScript
	// repo fully covered by the indexer would incorrectly
	// report partial language coverage. The limit should only
	// fire when there is actual evidence of unsupported-language
	// presence (e.g. a filesystem scan or a storage-side signal
	// that the agent crate does not currently have). Deferred
	// until that evidence is available. See docs/TECH-DEBT.md.
	Ok(AggregatorOutput {
		signals: vec![Signal::module_summary(evidence)],
		limits: vec![Limit::from_code(LimitCode::ModuleDataUnavailable)],
	})
}

/// File-scoped module summary.
///
/// Uses `compute_file_summary` to produce counts scoped to a
/// single file. Same output shape as the repo-level variant.
pub fn aggregate_file<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
	file_path: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
	let summary = storage.compute_file_summary(snapshot_uid, file_path)?;

	let evidence = ModuleSummaryEvidence {
		file_count: summary.file_count,
		symbol_count: summary.symbol_count,
		languages: summary.languages,
	};

	Ok(AggregatorOutput {
		signals: vec![Signal::module_summary(evidence)],
		limits: vec![Limit::from_code(LimitCode::ModuleDataUnavailable)],
	})
}

/// Path-scoped module summary.
///
/// Uses `compute_path_summary` to produce counts scoped to files
/// under a path prefix. Same output shape as the repo-level variant.
pub fn aggregate_path<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
	path_prefix: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
	let summary = storage.compute_path_summary(snapshot_uid, path_prefix)?;

	let evidence = ModuleSummaryEvidence {
		file_count: summary.file_count,
		symbol_count: summary.symbol_count,
		languages: summary.languages,
	};

	Ok(AggregatorOutput {
		signals: vec![Signal::module_summary(evidence)],
		limits: vec![Limit::from_code(LimitCode::ModuleDataUnavailable)],
	})
}
