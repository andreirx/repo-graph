//! Complexity aggregator.
//!
//! Queries cyclomatic complexity measurements and emits
//! `HIGH_COMPLEXITY` when symbols exceed the threshold.
//! Evidence includes the count and top N complex symbols.

use super::AggregatorOutput;
use crate::dto::budget::Budget;
use crate::dto::signal::{ComplexSymbolEvidence, ComplexityScope, HighComplexityEvidence, Signal};
use crate::errors::AgentStorageError;
use crate::storage_port::{AgentCancelCheck, AgentStorageRead};
use repo_graph_classification::is_vendored_path;

/// Default complexity threshold for HIGH_COMPLEXITY signal.
/// Symbols with cyclomatic complexity >= this value are flagged.
pub const DEFAULT_COMPLEXITY_THRESHOLD: u64 = 20;

/// "Fetch every above-threshold symbol" sentinel for the storage `limit` parameter.
///
/// TRUNCATION-AUDIT-1: `i64::MAX` is a valid SQLite `LIMIT` (no snapshot has 9.2e18 symbols), so
/// the adapter returns the FULL above-threshold set; we then sort + cut to the budget-derived cap
/// in the agent for a deterministic, source-independent top-N (see `aggregate_with_threshold`).
/// `usize::MAX` is NOT usable as the sentinel: rusqlite binds the limit as `i64` and errors on the
/// `u64::MAX` overflow, whereas `i64::MAX as usize` round-trips cleanly.
const FETCH_ALL: usize = i64::MAX as usize;

/// Aggregate complexity data and emit HIGH_COMPLEXITY if warranted.
///
/// Returns a signal when at least one symbol exceeds the threshold; empty when
/// no measurements exist or none exceed it. `budget` drives how many NAMED
/// centers ride in the evidence (ORIENT-DENSITY-1 §5, review-1 #2): lean at
/// `small`/`medium`, EVERY center at `large`/`--full` so the `--full` breakdown
/// is complete. `high_complexity_count` always reports the true total.
pub fn aggregate<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    budget: Budget,
) -> Result<AggregatorOutput, AgentStorageError> {
    // Production scope (`include_all = false`): the default orientation view.
    aggregate_cancellable(storage, snapshot_uid, budget, false, &mut || {
        std::ops::ControlFlow::Continue(())
    })
}

/// DAEMON-CANCEL-3: cancellable variant of [`aggregate`]. Threads `cancel` into the
/// FETCH_ALL complexity materialization (the demonstrated heavy chokepoint). The
/// daemon's orient handler passes a real checkpoint; `aggregate` passes a no-op.
pub fn aggregate_cancellable<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    budget: Budget,
    include_all: bool,
    cancel: AgentCancelCheck<'_>,
) -> Result<AggregatorOutput, AgentStorageError> {
    aggregate_with_threshold_cancellable(
        storage,
        snapshot_uid,
        DEFAULT_COMPLEXITY_THRESHOLD,
        budget,
        include_all,
        cancel,
    )
}

/// Aggregate with a custom threshold (for testing or configuration).
pub fn aggregate_with_threshold<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    threshold: u64,
    budget: Budget,
    include_all: bool,
) -> Result<AggregatorOutput, AgentStorageError> {
    aggregate_with_threshold_cancellable(
        storage,
        snapshot_uid,
        threshold,
        budget,
        include_all,
        &mut || std::ops::ControlFlow::Continue(()),
    )
}

/// DAEMON-CANCEL-3: cancellable variant of [`aggregate_with_threshold`]. This aggregator
/// makes exactly one storage read — the FETCH_ALL `query_high_complexity_symbols` read,
/// which is the checkpointed one — and derives `high_complexity_count` from the rows it
/// retains after scoping. It does NOT call `count_high_complexity_symbols`; that port
/// method remains only for the LiveGraph certificate, which compares the full storage set.
pub fn aggregate_with_threshold_cancellable<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    threshold: u64,
    budget: Budget,
    include_all: bool,
    cancel: AgentCancelCheck<'_>,
) -> Result<AggregatorOutput, AgentStorageError> {
    // COMPLEXITY-SCOPE-1 (RG-REQ-009-L01): read the FULL above-threshold set (unchanged
    // FETCH_ALL read — the set the LiveGraph certificate compares, P-CS-01), then scope
    // it HERE, agent-side. The aggregator no longer calls `count_high_complexity_symbols`
    // (which counts the UNFILTERED set): `high_complexity_count` must be the count of the
    // SCOPED set, so it is derived from the retained rows below. The port method stays for
    // the LiveGraph certificate, which deliberately compares the full storage read.
    //
    // TRUNCATION-AUDIT-1: the top-N cut is owned here (a TOTAL sort by complexity DESC then
    // the unique stable_key) so the sample is a pure function of the SET, not of storage row
    // order. The cost-optimal `ORDER BY … LIMIT N` in SQL is recorded as a follow-up.
    let mut rows = storage.query_high_complexity_symbols_cancellable(
        snapshot_uid,
        threshold,
        FETCH_ALL,
        cancel,
    )?;

    // The UNFILTERED read being empty means no symbol exceeds the threshold — no signal
    // (the former `count == 0` early return). When it is non-empty the signal is ALWAYS
    // emitted, even if scoping excludes every row, so the reader is told what was set aside.
    if rows.is_empty() {
        return Ok(AggregatorOutput::empty());
    }

    // Scope the ranking. `--include-all` keeps every above-threshold symbol; the default
    // production scope drops generated/vendored/test symbols (decided from the persisted
    // `is_test`/`is_generated` file facts and the ONE `is_vendored_path` predicate) and
    // reports how many it set aside. A row with no owning file (`file_path == None`) carries
    // no persisted fact, so it is KEPT — never a fabricated exclusion.
    let scope = if include_all {
        ComplexityScope::All
    } else {
        let before = rows.len();
        rows.retain(|m| {
            let vendored = m
                .file_path
                .as_deref()
                .map(is_vendored_path)
                .unwrap_or(false);
            !(m.is_test || m.is_generated || vendored)
        });
        ComplexityScope::Production {
            excluded_count: (before - rows.len()) as u64,
        }
    };

    // The honest total is the count of the SCOPED set (RG-REQ-009-L01), never the storage
    // count of the unfiltered set.
    let high_complexity_count = rows.len() as u64;

    crate::ordering::sort_complexity(&mut rows);
    // ORIENT-DENSITY-1 §5: budget trades DEPTH — lean top-N at small/medium,
    // EVERY center (cap usize::MAX) at large/--full for a complete breakdown.
    rows.truncate(budget.max_complexity_centers());

    let top: Vec<ComplexSymbolEvidence> = rows
        .into_iter()
        .map(|m| ComplexSymbolEvidence {
            symbol: m.symbol_name,
            file: m.file_path,
            // ANCHORS-EVERYWHERE-1: line shares the SQLite `nodes` row with `file`.
            line: m.line,
            complexity: m.complexity,
        })
        .collect();

    let evidence = HighComplexityEvidence {
        high_complexity_count,
        threshold,
        top_complex: top,
        scope,
    };

    Ok(AggregatorOutput {
        signals: vec![Signal::high_complexity(evidence)],
        limits: Vec::new(),
    })
}

#[cfg(test)]
#[path = "complexity_tests.rs"]
mod tests;
