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
use crate::dto::budget::Budget;
use crate::dto::limit::{DegradationInfo, Limit, LimitCode};
use crate::dto::signal::{
    ModuleKindBreakdown, ModuleSizeEvidence, ModuleSummaryEvidence, PackageGroupEvidence, Signal,
};
use crate::errors::AgentStorageError;
use crate::package_groups::{rollup_package_groups, DirGroup};
use crate::storage_port::AgentStorageRead;

/// Read the directory TOPOLOGY (`nodes` kind=MODULE ⋈ OWNS leaf dirs) and fold
/// it into the logical package groups the dense `orient` headline NAMES
/// (MODULE-MODEL-1 D2(i)/D4).
///
/// Independent of `module_candidates`: present whenever files were indexed, so
/// the structure is named even on the Rust-indexer path where
/// `get_module_summary` returns `None`. Empty only when no directory owns files.
fn read_package_groups<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
) -> Result<Vec<PackageGroupEvidence>, AgentStorageError> {
    let dirs: Vec<DirGroup> = storage
        .list_directory_groups(snapshot_uid)?
        .into_iter()
        .map(|g| DirGroup {
            path: g.path,
            file_count: g.file_count,
        })
        .collect();
    Ok(rollup_package_groups(&dirs)
        .into_iter()
        .map(|g| PackageGroupEvidence {
            name: g.name,
            file_count: g.file_count,
            test_file_count: g.test_file_count,
        })
        .collect())
}

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
    budget: Budget,
) -> Result<AggregatorOutput, AgentStorageError> {
    // Always get raw snapshot totals.
    let summary = storage.compute_repo_summary(snapshot_uid)?;

    // Directory/package TOPOLOGY (Layer 0/1) — the structure the headline NAMES.
    // Read independently of module_candidates so the named structure survives on
    // repos where get_module_summary returns None (Rust-indexer path). Only one
    // match arm runs, so moving it into each is fine.
    let package_groups = read_package_groups(storage, snapshot_uid)?;

    // Check for module discovery data (the declared/inferred `module_candidates`
    // notion — a SEPARATE, labelled count, never collapsed into the topology).
    let module_summary = storage.get_module_summary(snapshot_uid)?;

    let (evidence, limits) = match module_summary {
        Some(ms) => {
            // Module discovery data exists — include it, no limit.
            // ORIENT-DENSITY-1 §5: pull the NAMED top modules by size for the
            // dense structure headline. The budget drives DEPTH — `small`/
            // `medium` request a bounded headline set, `large`/`--full` request
            // the COMPLETE list so the `--full` "Modules (by size)" breakdown is
            // genuinely full. The storage read applies the cap (a total,
            // source-independent prefix); `discovered_module_count` below still
            // reports the true total, so a bounded cap never overclaims.
            let top_modules = storage
                .list_module_sizes(snapshot_uid, budget.max_modules())?
                .into_iter()
                .map(|m| ModuleSizeEvidence {
                    path: m.path,
                    file_count: m.file_count,
                })
                .collect();
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
                top_modules,
                package_groups,
            };
            (evidence, Vec::new())
        }
        None => {
            // Fallback: no module candidates, emit limit. The package GROUPS
            // (directory topology) are still surfaced — they do not depend on
            // module_candidates — so the structure is named even here.
            let evidence = ModuleSummaryEvidence {
                file_count: summary.file_count,
                symbol_count: summary.symbol_count,
                languages: summary.languages,
                discovered_module_count: None,
                module_kinds: None,
                top_modules: Vec::new(),
                package_groups,
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
    // to indicate this is a fallback path. Package groups are a repo-wide
    // topology, not file-scoped → empty here.
    let evidence = ModuleSummaryEvidence {
        file_count: summary.file_count,
        symbol_count: summary.symbol_count,
        languages: summary.languages,
        discovered_module_count: None,
        module_kinds: None,
        top_modules: Vec::new(),
        package_groups: Vec::new(),
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
    // Module ownership is per-repo. Emit the limit. Package groups are a
    // repo-wide topology, not path-scoped → empty here.
    let evidence = ModuleSummaryEvidence {
        file_count: summary.file_count,
        symbol_count: summary.symbol_count,
        languages: summary.languages,
        discovered_module_count: None,
        module_kinds: None,
        top_modules: Vec::new(),
        package_groups: Vec::new(),
    };

    Ok(AggregatorOutput {
        signals: vec![Signal::module_summary(evidence)],
        limits: vec![module_data_unavailable_limit()],
    })
}
