//! Repo-level orient pipeline.
//!
//! Orchestrates aggregators, ranking, budget truncation, and
//! confidence derivation into one `OrientResult`.
//!
//! Pipeline order (deterministic):
//!
//!   1. Resolve repo identity (`get_repo`).
//!   2. Resolve latest READY snapshot (`get_latest_snapshot`).
//!   3. Run aggregators in a fixed order:
//!      snapshot → trust → cycles → boundary → dead_code → module_summary.
//!      The order of aggregator invocation has no effect on the
//!      final signal order — the ranking pass re-sorts everything
//!      — but fixing it keeps error-propagation deterministic
//!      and makes test fixtures predictable.
//!   4. Collect all signals and limits into single vectors.
//!   5. Append the `GATE_UNAVAILABLE` limit (Rust-42 policy
//!      stub — see Sub-Decision A1 and TECH-DEBT.md).
//!   6. Append the `COMPLEXITY_UNAVAILABLE` limit
//!      (HIGH_COMPLEXITY signal is not emitted in Rust-42 because
//!      the Rust indexer does not produce cyclomatic measurements).
//!   7. Sort signals + assign ranks.
//!   8. Truncate signals / limits to budget caps.
//!   9. Derive confidence from raw trust data (not from signals).
//!  10. Build the envelope.

use crate::aggregators;
use crate::aggregators::AggregatorOutput;
use crate::confidence::derive_repo_confidence;
use repo_graph_gate::GateStorageRead;

use crate::dto::budget::Budget;
use crate::dto::envelope::{
	Focus, OrientResult, ORIENT_COMMAND, ORIENT_SCHEMA,
};
use crate::dto::limit::{Limit, LimitCode};
use crate::dto::signal::Signal;
use crate::errors::OrientError;
use crate::ranking;
use crate::storage_port::AgentStorageRead;

/// Repo-level orient pipeline.
///
/// `now` is an ISO 8601 timestamp used by the gate aggregator to
/// evaluate waiver expiry through `find_active_waivers`. The
/// orient use case is clock-explicit: it never touches the
/// system clock. Callers (CLI, daemon, tests) must supply a
/// wall-clock value. A wrong `now` produces wrong gate outcomes
/// at orient time, so this parameter is not optional.
///
/// See `docs/TECH-DEBT.md` and
/// `docs/architecture/agent-orientation-contract.md` for the
/// rationale; the previous `AGENT_NOW_SENTINEL` constant was
/// removed in the P2 fix because a far-future or far-past
/// sentinel silently mis-evaluates finite-expiry waivers.
pub fn orient_repo<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	budget: Budget,
	now: &str,
) -> Result<OrientResult, OrientError> {
	// ── 1. Resolve repo identity. ────────────────────────────
	let repo = storage
		.get_repo(repo_uid)?
		.ok_or_else(|| OrientError::NoRepo { repo_uid: repo_uid.to_string() })?;

	// ── 2. Resolve snapshot. ─────────────────────────────────
	let snapshot = storage
		.get_latest_snapshot(repo_uid)?
		.ok_or_else(|| OrientError::NoSnapshot {
			repo_uid: repo_uid.to_string(),
		})?;

	let snapshot_uid = snapshot.snapshot_uid.clone();

	// ── 3. Run aggregators. ──────────────────────────────────
	let mut all_signals: Vec<Signal> = Vec::new();
	let mut all_limits: Vec<Limit> = Vec::new();

	// snapshot_info
	let snap_out = aggregators::snapshot::aggregate(&snapshot);
	merge(&mut all_signals, &mut all_limits, snap_out);

	// trust (returns summary + stale flag for confidence)
	let trust_result =
		aggregators::trust::aggregate(storage, repo_uid, &snapshot_uid)?;
	merge(&mut all_signals, &mut all_limits, trust_result.output);

	// cycles
	let cycles_out = aggregators::cycles::aggregate(storage, &snapshot_uid)?;
	merge(&mut all_signals, &mut all_limits, cycles_out);

	// boundary
	let boundary_out =
		aggregators::boundary::aggregate(storage, repo_uid, &snapshot_uid)?;
	merge(&mut all_signals, &mut all_limits, boundary_out);

	// dead_code — reliability-gated. The aggregator reads the
	// trust layer's composite `dead_code_reliability` verdict
	// and suppresses the signal (emitting a
	// DEAD_CODE_UNRELIABLE limit instead) when the level is
	// not High. The agent crate does NOT re-derive the
	// threshold logic; the trust crate is the authority. See
	// the dead_code module doc for the rationale, and
	// `docs/spikes/2026-04-15-orient-on-repo-graph.md` for the
	// spike that motivated this gate.
	let dead_out = aggregators::dead_code::aggregate(
		storage,
		&snapshot_uid,
		repo_uid,
		&trust_result.summary,
	)?;
	merge(&mut all_signals, &mut all_limits, dead_out);

	// module_summary
	let mod_out = aggregators::module_summary::aggregate(storage, &snapshot_uid)?;
	merge(&mut all_signals, &mut all_limits, mod_out);

	// gate — emits at most one of GATE_PASS / GATE_FAIL /
	// GATE_INCOMPLETE, or the GATE_NOT_CONFIGURED limit when
	// the repo has no active requirement declarations.
	// Relocated from rgr/src/gate.rs in Rust-43A and called
	// here through the GateStorageRead supertrait bound on S.
	// `now` is forwarded directly to gate's waiver-expiry
	// filter. See the function doc for the clock-explicit
	// contract.
	let gate_out = aggregators::gate::aggregate(
		storage,
		repo_uid,
		&snapshot_uid,
		now,
	)?;
	merge(&mut all_signals, &mut all_limits, gate_out);

	// ── 4 & 5. Static limits from Rust-42 scope. ─────────────
	// COMPLEXITY_UNAVAILABLE is added unconditionally because
	// the Rust indexer does not emit cyclomatic measurements
	// and the agent pipeline does not attempt to synthesize
	// them. Orient reports "unknown", never "none".
	all_limits.push(Limit::from_code(LimitCode::ComplexityUnavailable));

	// ── 7. Sort + rank. ──────────────────────────────────────
	ranking::sort_and_rank(&mut all_signals);

	// ── 8. Truncate. ─────────────────────────────────────────
	let sig_tx = ranking::truncate_signals(&mut all_signals, budget);
	let lim_tx = ranking::truncate_limits(&mut all_limits, budget);

	// ── 9. Confidence. ───────────────────────────────────────
	let confidence =
		derive_repo_confidence(&trust_result.summary, trust_result.stale);

	// ── 10. Build envelope. ──────────────────────────────────
	let truncated_any = sig_tx.truncated || lim_tx.truncated;

	Ok(OrientResult {
		schema: ORIENT_SCHEMA,
		command: ORIENT_COMMAND,
		repo: repo.name,
		snapshot: snapshot_uid,
		focus: Focus::repo(&repo.repo_uid),
		confidence,

		signals: all_signals,
		signals_truncated: sig_tx.truncated.then_some(true),
		signals_omitted_count: sig_tx.truncated.then_some(sig_tx.omitted),

		limits: all_limits,
		limits_truncated: lim_tx.truncated.then_some(true),
		limits_omitted_count: lim_tx.truncated.then_some(lim_tx.omitted),

		next: Vec::new(),
		next_truncated: None,
		next_omitted_count: None,

		truncated: truncated_any,
	})
}

fn merge(signals: &mut Vec<Signal>, limits: &mut Vec<Limit>, out: AggregatorOutput) {
	signals.extend(out.signals);
	limits.extend(out.limits);
}
