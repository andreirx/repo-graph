//! Complexity aggregator.
//!
//! Queries cyclomatic complexity measurements and emits
//! `HIGH_COMPLEXITY` when symbols exceed the threshold.
//! Evidence includes the count and top N complex symbols.

use super::AggregatorOutput;
use crate::dto::budget::Budget;
use crate::dto::signal::{ComplexSymbolEvidence, HighComplexityEvidence, Signal};
use crate::errors::AgentStorageError;
use crate::storage_port::{AgentCancelCheck, AgentStorageRead};

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
    aggregate_cancellable(storage, snapshot_uid, budget, &mut || {
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
    cancel: AgentCancelCheck<'_>,
) -> Result<AggregatorOutput, AgentStorageError> {
    aggregate_with_threshold_cancellable(
        storage,
        snapshot_uid,
        DEFAULT_COMPLEXITY_THRESHOLD,
        budget,
        cancel,
    )
}

/// Aggregate with a custom threshold (for testing or configuration).
pub fn aggregate_with_threshold<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    threshold: u64,
    budget: Budget,
) -> Result<AggregatorOutput, AgentStorageError> {
    aggregate_with_threshold_cancellable(storage, snapshot_uid, threshold, budget, &mut || {
        std::ops::ControlFlow::Continue(())
    })
}

/// DAEMON-CANCEL-3: cancellable variant of [`aggregate_with_threshold`]. Only the
/// FETCH_ALL `query_high_complexity_symbols` read is checkpointed — `count_*` is a
/// single fast aggregate (not a materialization), left alone per the NARROW scope.
pub fn aggregate_with_threshold_cancellable<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    threshold: u64,
    budget: Budget,
    cancel: AgentCancelCheck<'_>,
) -> Result<AggregatorOutput, AgentStorageError> {
    // Get the true count of symbols exceeding threshold (not limited)
    let count = storage.count_high_complexity_symbols(snapshot_uid, threshold)?;

    if count == 0 {
        return Ok(AggregatorOutput::empty());
    }

    // TRUNCATION-AUDIT-1: fetch the FULL above-threshold set, then apply a TOTAL deterministic
    // sort (complexity DESC, then the unique stable_key) and cut to the budget cap HERE in the
    // agent. We deliberately do NOT accept storage's `ORDER BY complexity DESC LIMIT N` cut: its
    // ties at the cut boundary fall to SQLite rowid order, so the surviving sample would depend on
    // storage row order rather than on the SET (the DR-EXPLAIN-CALLER-ORDER hazard). Owning the cut
    // here makes the top-N sample a pure function of the above-threshold set — identical regardless
    // of which store answered. Cost (honest): the table SCAN is already paid — `count_*` above passes
    // over the same `cyclomatic_complexity` measurements. The added work is MATERIALISING the rows
    // (`query_*` joins nodes+files and JSON-parses each, and filters the threshold in Rust, so it
    // materialises every complexity row, not just the above-threshold minority) to pick the
    // budget-capped top-N. This is bounded and paid once per orient (not a hot loop); the
    // cost-OPTIMAL fix — a total `ORDER BY complexity DESC, target_stable_key LIMIT N` in the storage
    // SQL so the LIMIT cut is itself deterministic — lives in the storage adapter, outside this
    // slice's file scope, and is recorded as the follow-up. Determinism here is required NOW and is
    // achievable agent-side.
    let mut high_complexity = storage.query_high_complexity_symbols_cancellable(
        snapshot_uid,
        threshold,
        FETCH_ALL,
        cancel,
    )?;
    crate::ordering::sort_complexity(&mut high_complexity);
    // ORIENT-DENSITY-1 §5: budget trades DEPTH — lean top-N at small/medium,
    // EVERY center (cap usize::MAX) at large/--full for a complete breakdown.
    high_complexity.truncate(budget.max_complexity_centers());

    let top: Vec<ComplexSymbolEvidence> = high_complexity
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
        high_complexity_count: count,
        threshold,
        top_complex: top,
    };

    Ok(AggregatorOutput {
        signals: vec![Signal::high_complexity(evidence)],
        limits: Vec::new(),
    })
}

#[cfg(test)]
#[path = "complexity_tests.rs"]
mod tests;
