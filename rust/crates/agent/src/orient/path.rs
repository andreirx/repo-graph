//! Path-area (subtree) orient pipeline.
//!
//! Emits:
//!   - `DEAD_CODE` / `DEAD_CODE_UNRELIABLE` — scoped to the
//!     path prefix via `find_dead_nodes_in_path`.
//!   - `MODULE_SUMMARY` — scoped via `compute_path_summary`.
//!   - `BOUNDARY_VIOLATIONS` — scoped to declarations whose
//!     source module is under the prefix.
//!   - `IMPORT_CYCLES` — scoped to cycles involving modules
//!     under the prefix.
//!   - Gate signals — obligations filtered by target prefix.
//!   - Trust signals — repo-wide, unchanged.
//!   - `SNAPSHOT_INFO` — informational, unchanged.
//!   - Static limits: `MODULE_DATA_UNAVAILABLE`,
//!     `COMPLEXITY_UNAVAILABLE`.

use crate::aggregators;
use crate::aggregators::AggregatorOutput;
use crate::confidence::derive_repo_confidence;
use crate::doc_relevance::{DocEntry, DocFocusContext, select_relevant_docs};
use repo_graph_gate::GateStorageRead;

use crate::dto::budget::Budget;
use crate::dto::envelope::{
	DocumentationSection, Focus, OrientResult, ORIENT_COMMAND, ORIENT_SCHEMA,
};
use crate::dto::limit::{Limit, LimitCode};
use crate::dto::signal::Signal;
use crate::errors::OrientError;
use crate::ranking;
use crate::storage_port::{AgentSnapshot, AgentStorageRead};

/// Path-area orient pipeline.
///
/// `path_prefix` is the repo-relative directory prefix.
/// `module_stable_key` is the MODULE node's stable key when one
/// exists at the exact prefix path. `None` when the prefix has
/// content but no MODULE node.
pub fn orient_path<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_name: &str,
	snapshot: &AgentSnapshot,
	path_prefix: &str,
	module_stable_key: Option<&str>,
	budget: Budget,
	now: &str,
) -> Result<OrientResult, OrientError> {
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

	// ── cycles (path-scoped) ────────────────────────────────
	let cycles_out = aggregators::cycles::aggregate_path(
		storage,
		snapshot_uid,
		path_prefix,
	)?;
	merge(&mut all_signals, &mut all_limits, cycles_out);

	// ── boundary (path-scoped) ──────────────────────────────
	let boundary_out = aggregators::boundary::aggregate_path(
		storage,
		repo_uid,
		snapshot_uid,
		path_prefix,
	)?;
	merge(&mut all_signals, &mut all_limits, boundary_out);

	// ── dead_code (path-scoped) ─────────────────────────────
	let dead_out = aggregators::dead_code::aggregate_path(
		storage,
		snapshot_uid,
		repo_uid,
		path_prefix,
		&trust_result.summary,
	)?;
	merge(&mut all_signals, &mut all_limits, dead_out);

	// ── module_summary (path-scoped) ────────────────────────
	let mod_out = aggregators::module_summary::aggregate_path(
		storage,
		snapshot_uid,
		path_prefix,
	)?;
	merge(&mut all_signals, &mut all_limits, mod_out);

	// ── gate (path-scoped) ──────────────────────────────────
	let gate_out = aggregators::gate::aggregate_path(
		storage,
		repo_uid,
		snapshot_uid,
		now,
		path_prefix,
	)?;
	merge(&mut all_signals, &mut all_limits, gate_out);

	// ── static limits ───────────────────────────────────────
	all_limits.push(Limit::from_code(LimitCode::ComplexityUnavailable));

	// ── ranking + truncation ────────────────────────────────
	ranking::sort_and_rank(&mut all_signals);
	let sig_tx = ranking::truncate_signals(&mut all_signals, budget);
	let lim_tx = ranking::truncate_limits(&mut all_limits, budget);

	// ── confidence ──────────────────────────────────────────
	let confidence =
		derive_repo_confidence(&trust_result.summary, trust_result.stale);

	// ── documentation (docs-primary pivot) ──────────────────
	let documentation = build_documentation_section(storage, repo_uid, path_prefix);

	// ── envelope ────────────────────────────────────────────
	let truncated_any = sig_tx.truncated || lim_tx.truncated;

	Ok(OrientResult {
		schema: ORIENT_SCHEMA,
		command: ORIENT_COMMAND,
		repo: repo_name.to_string(),
		snapshot: snapshot_uid.clone(),
		focus: Focus::path_area(path_prefix, module_stable_key, path_prefix),
		confidence,

		documentation,

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

/// Build the documentation section for path-scoped orient.
fn build_documentation_section<S: AgentStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	path_prefix: &str,
) -> Option<DocumentationSection> {
	let agent_entries = match storage.get_doc_inventory(repo_uid) {
		Ok(entries) => entries,
		Err(_) => return None,
	};

	if agent_entries.is_empty() {
		return None;
	}

	let inventory: Vec<DocEntry> = agent_entries
		.into_iter()
		.map(|e| DocEntry {
			path: e.path,
			kind: e.kind,
			generated: e.generated,
		})
		.collect();

	let focus = DocFocusContext::path(path_prefix);
	let relevant = select_relevant_docs(&inventory, &focus);

	if relevant.is_empty() {
		return None;
	}

	let count = relevant.len();
	Some(DocumentationSection {
		relevant_files: relevant,
		count,
	})
}
