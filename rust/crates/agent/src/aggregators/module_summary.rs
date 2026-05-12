//! Repo-level structural summary.
//!
//! Emits `MODULE_SUMMARY` with snapshot-level totals and, when
//! available, module discovery data.
//!
//! Behavior:
//!
//!   - If `module_candidates` table has data: include module
//!     evidence (`discovered_module_count`, `module_kinds`), do NOT
//!     emit `MODULE_DATA_UNAVAILABLE`.
//!
//!   - If `module_candidates` is empty: fall back to raw snapshot
//!     totals, emit `MODULE_DATA_UNAVAILABLE` limit so the agent
//!     can distinguish "no discovered modules" from "module
//!     discovery data is not queryable from Rust".
//!
//! The signal itself is NEVER suppressed by the limit — they are
//! orthogonal. The signal says "here is what the snapshot contains",
//! the limit says "module discovery layer is unavailable".

use super::AggregatorOutput;
use crate::dto::limit::{DegradationInfo, Limit, LimitCode};
use crate::dto::signal::{ModuleKindBreakdown, ModuleSummaryEvidence, Signal};
use crate::errors::AgentStorageError;
use crate::storage_port::AgentStorageRead;

/// Create the standard MODULE_DATA_UNAVAILABLE limit with degradation info.
///
/// This is the canonical degradation for module discovery on the Rust indexer
/// path, where module_candidates is not populated.
fn module_data_unavailable_limit() -> Limit {
    Limit::from_code_with_degradation(
        LimitCode::ModuleDataUnavailable,
        DegradationInfo::unsupported(
            "ModuleCandidates",
            "module_candidates table is not populated on Rust indexer path",
        ),
    )
}

pub fn aggregate<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
    // Always get raw snapshot totals.
    let summary = storage.compute_repo_summary(snapshot_uid)?;

    // Check for module discovery data.
    let module_summary = storage.get_module_summary(snapshot_uid)?;

    let (evidence, limits) = match module_summary {
        Some(ms) => {
            // Module discovery data exists — include it, no limit.
            let evidence = ModuleSummaryEvidence {
                file_count: summary.file_count,
                symbol_count: summary.symbol_count,
                languages: summary.languages,
                discovered_module_count: Some(ms.discovered_module_count),
                module_kinds: Some(ModuleKindBreakdown {
                    declared: ms.declared_count,
                    operational: ms.operational_count,
                    inferred: ms.inferred_count,
                }),
            };
            (evidence, Vec::new())
        }
        None => {
            // Fallback: no module candidates, emit limit.
            let evidence = ModuleSummaryEvidence {
                file_count: summary.file_count,
                symbol_count: summary.symbol_count,
                languages: summary.languages,
                discovered_module_count: None,
                module_kinds: None,
            };
            let limits = vec![module_data_unavailable_limit()];
            (evidence, limits)
        }
    };

    Ok(AggregatorOutput {
        signals: vec![Signal::module_summary(evidence)],
        limits,
    })
}

/// File-scoped module summary.
///
/// Uses `compute_file_summary` to produce counts scoped to a
/// single file. Module discovery data is not file-scoped, so
/// this always returns the fallback shape with the limit.
pub fn aggregate_file<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    file_path: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
    let summary = storage.compute_file_summary(snapshot_uid, file_path)?;

    // File-level summary does not include module discovery data.
    // Module ownership is per-repo, not per-file. Emit the limit
    // to indicate this is a fallback path.
    let evidence = ModuleSummaryEvidence {
        file_count: summary.file_count,
        symbol_count: summary.symbol_count,
        languages: summary.languages,
        discovered_module_count: None,
        module_kinds: None,
    };

    Ok(AggregatorOutput {
        signals: vec![Signal::module_summary(evidence)],
        limits: vec![module_data_unavailable_limit()],
    })
}

/// Path-scoped module summary.
///
/// Uses `compute_path_summary` to produce counts scoped to files
/// under a path prefix. Module discovery data is repo-scoped,
/// not path-scoped, so this always returns the fallback shape
/// with the limit.
pub fn aggregate_path<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    path_prefix: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
    let summary = storage.compute_path_summary(snapshot_uid, path_prefix)?;

    // Path-level summary does not include module discovery data.
    // Module ownership is per-repo. Emit the limit.
    let evidence = ModuleSummaryEvidence {
        file_count: summary.file_count,
        symbol_count: summary.symbol_count,
        languages: summary.languages,
        discovered_module_count: None,
        module_kinds: None,
    };

    Ok(AggregatorOutput {
        signals: vec![Signal::module_summary(evidence)],
        limits: vec![module_data_unavailable_limit()],
    })
}
