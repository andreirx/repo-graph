//! File-scoped orient pipeline.
//!
//! Emits a narrower set of signals than the repo-level pipeline:
//!
//!   - `DEAD_CODE` / `DEAD_CODE_UNRELIABLE` — scoped to the
//!     single file via `find_dead_nodes_in_file`.
//!   - `MODULE_SUMMARY` — scoped via `compute_file_summary`.
//!   - Trust signals — repo-wide, unchanged.
//!   - `SNAPSHOT_INFO` — informational, unchanged.
//!   - Static limits: `MODULE_DATA_UNAVAILABLE`,
//!     `COMPLEXITY_UNAVAILABLE`.
//!
//! Does NOT emit: `BOUNDARY_VIOLATIONS`, `IMPORT_CYCLES`, gate
//! signals. These are not meaningful at single-file granularity.

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
use crate::storage_port::{AgentSnapshot, AgentStorageRead};

/// File-scoped orient pipeline.
///
/// `file_path` is the repo-relative path of the file. It must
/// have been validated as an existing FILE node by the caller.
/// `file_stable_key` is the FILE node's stable key, if resolved.
pub fn orient_file<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_name: &str,
	snapshot: &AgentSnapshot,
	file_path: &str,
	file_stable_key: Option<&str>,
	budget: Budget,
	now: &str,
) -> Result<OrientResult, OrientError> {
	let _ = now; // clock parameter reserved for future use
	let snapshot_uid = &snapshot.snapshot_uid;
	let repo_uid = &snapshot.repo_uid;

	let mut all_signals: Vec<Signal> = Vec::new();
	let mut all_limits: Vec<Limit> = Vec::new();

	// ── snapshot_info ────────────────────────────────────────
	let snap_out = aggregators::snapshot::aggregate(snapshot);
	merge(&mut all_signals, &mut all_limits, snap_out);

	// ── trust (repo-wide) ───────────────────────────────────
	let trust_result =
		aggregators::trust::aggregate(storage, repo_uid, snapshot_uid)?;
	merge(&mut all_signals, &mut all_limits, trust_result.output);

	// ── dead_code (file-scoped) ─────────────────────────────
	let dead_out = aggregators::dead_code::aggregate_file(
		storage,
		snapshot_uid,
		repo_uid,
		file_path,
		&trust_result.summary,
	)?;
	merge(&mut all_signals, &mut all_limits, dead_out);

	// ── module_summary (file-scoped) ────────────────────────
	let mod_out = aggregators::module_summary::aggregate_file(
		storage,
		snapshot_uid,
		file_path,
	)?;
	merge(&mut all_signals, &mut all_limits, mod_out);

	// ── static limits ───────────────────────────────────────
	all_limits.push(Limit::from_code(LimitCode::ComplexityUnavailable));

	// ── ranking + truncation ────────────────────────────────
	ranking::sort_and_rank(&mut all_signals);
	let sig_tx = ranking::truncate_signals(&mut all_signals, budget);
	let lim_tx = ranking::truncate_limits(&mut all_limits, budget);

	// ── confidence ──────────────────────────────────────────
	let confidence =
		derive_repo_confidence(&trust_result.summary, trust_result.stale);

	// ── envelope ────────────────────────────────────────────
	let truncated_any = sig_tx.truncated || lim_tx.truncated;

	let focus = Focus::file(file_path, file_stable_key, file_path);

	Ok(OrientResult {
		schema: ORIENT_SCHEMA,
		command: ORIENT_COMMAND,
		repo: repo_name.to_string(),
		snapshot: snapshot_uid.clone(),
		focus,
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

fn merge(
	signals: &mut Vec<Signal>,
	limits: &mut Vec<Limit>,
	out: AggregatorOutput,
) {
	signals.extend(out.signals);
	limits.extend(out.limits);
}
