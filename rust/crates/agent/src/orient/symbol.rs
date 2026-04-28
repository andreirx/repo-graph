//! Symbol-scoped orient pipeline.
//!
//! Emits:
//!   - `CALLERS_SUMMARY` — direct callers grouped by owning module.
//!   - `CALLEES_SUMMARY` — direct callees grouped by owning module.
//!   - `DEAD_CODE` — whether this symbol is dead (file-scoped
//!     dead-node check, same reliability gate).
//!   - Trust signals — repo-wide, unchanged.
//!   - `SNAPSHOT_INFO` — informational, unchanged.
//!   - Static limits: `COMPLEXITY_UNAVAILABLE`.
//!
//! Does NOT emit `MODULE_DATA_UNAVAILABLE` — the symbol pipeline
//! intentionally omits `MODULE_SUMMARY`, so the limit that caveats
//! module-summary data has no surface to apply to.
//!
//! Inherited module-context signals (only when the symbol has an
//! owning module via OWNS edges):
//!   - `BOUNDARY_VIOLATIONS` — scoped to the owning module (exact
//!     match, not prefix).
//!   - `IMPORT_CYCLES` — scoped to the owning module (exact match).
//!   - Gate signals — obligations filtered by owning module target
//!     (exact match).
//!
//! All inherited signals are tagged with
//! `SignalScope::ModuleContext` via `.with_module_context()`.
//!
//! Does NOT emit: `MODULE_SUMMARY` (a 1-symbol summary is not
//! meaningful at symbol scope).

use std::collections::HashMap;

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
use crate::dto::signal::{
	CallersSummaryEvidence, CalleesSummaryEvidence, ModuleCountEvidence,
	Signal,
};
use crate::errors::OrientError;
use crate::ranking;
use crate::storage_port::{
	AgentSnapshot, AgentStorageRead, AgentSymbolContext,
};

/// Maximum number of top modules surfaced in callers/callees
/// summary evidence.
const TOP_MODULES_N: usize = 3;

/// Symbol-scoped orient pipeline.
///
/// `symbol_stable_key` is the resolved stable key of the SYMBOL
/// node. `context` is the symbol's owning-file and owning-module
/// context from `get_symbol_context`.
/// `focus_input` is the original focus string the caller supplied
/// (e.g. the stable key, the symbol name, or whatever the user
/// typed). It is carried verbatim into `Focus::symbol.input` so
/// the caller can see exactly what query was resolved.
pub fn orient_symbol<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_name: &str,
	snapshot: &AgentSnapshot,
	symbol_stable_key: &str,
	context: &AgentSymbolContext,
	focus_input: &str,
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

	// ── callers_summary ─────────────────────────────────────
	let callers = storage.find_symbol_callers(snapshot_uid, symbol_stable_key)?;
	if !callers.is_empty() {
		let evidence = build_callers_evidence(&callers);
		all_signals.push(Signal::callers_summary(evidence));
	}

	// ── callees_summary ─────────────────────────────────────
	let callees = storage.find_symbol_callees(snapshot_uid, symbol_stable_key)?;
	if !callees.is_empty() {
		let evidence = build_callees_evidence(&callees);
		all_signals.push(Signal::callees_summary(evidence));
	}

	// dead_code signal removed: `rmap dead` surface is disabled.
	// Internal substrate preserved in storage, but not surfaced.

	// ── inherited module-context signals ────────────────────
	if let Some(ref module_path) = context.module_path {
		// ── boundary_violations (module-scoped) ─────────────
		let boundary_out = aggregate_boundary_for_module(
			storage,
			repo_uid,
			snapshot_uid,
			module_path,
		)?;
		for sig in boundary_out.signals {
			all_signals.push(sig.with_module_context());
		}
		all_limits.extend(boundary_out.limits);

		// ── import_cycles (module-scoped) ────────────────────
		let cycles_out = aggregate_cycles_for_module(
			storage,
			snapshot_uid,
			module_path,
		)?;
		for sig in cycles_out.signals {
			all_signals.push(sig.with_module_context());
		}
		all_limits.extend(cycles_out.limits);

		// ── gate (module-scoped, exact match) ───────────────
		let gate_out = aggregate_gate_for_module(
			storage,
			repo_uid,
			snapshot_uid,
			now,
			module_path,
		)?;
		for sig in gate_out.signals {
			all_signals.push(sig.with_module_context());
		}
		all_limits.extend(gate_out.limits);
	}

	// ── static limits ───────────────────────────────────────
	// COMPLEXITY_UNAVAILABLE: the Rust indexer does not produce
	// cyclomatic measurements; relevant if HIGH_COMPLEXITY is
	// ever activated at symbol scope.
	all_limits.push(Limit::from_code(LimitCode::ComplexityUnavailable));
	// MODULE_DATA_UNAVAILABLE is NOT emitted at symbol scope.
	// MODULE_SUMMARY is intentionally absent (a 1-symbol
	// degenerate summary is not meaningful), so the limit that
	// says "module discovery data is unavailable" has no surface
	// it would caveat. Emitting it would be unconditional noise.

	// ── ranking + truncation ────────────────────────────────
	ranking::sort_and_rank(&mut all_signals);
	let sig_tx = ranking::truncate_signals(&mut all_signals, budget);
	let lim_tx = ranking::truncate_limits(&mut all_limits, budget);

	// ── confidence ──────────────────────────────────────────
	let confidence =
		derive_repo_confidence(&trust_result.summary, trust_result.stale);

	// ── documentation (docs-primary pivot) ──────────────────
	// Symbol focus uses the file path for doc relevance (same as file focus).
	let documentation = build_documentation_section(
		storage,
		repo_uid,
		context.file_path.as_deref(),
	);

	// ── envelope ────────────────────────────────────────────
	let truncated_any = sig_tx.truncated || lim_tx.truncated;

	let focus = Focus::symbol(
		focus_input,
		symbol_stable_key,
		context.file_path.as_deref(),
	);

	Ok(OrientResult {
		schema: ORIENT_SCHEMA,
		command: ORIENT_COMMAND,
		repo: repo_name.to_string(),
		snapshot: snapshot_uid.clone(),
		focus,
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

// ── Callers/callees evidence builders ───────────────────────────

fn build_callers_evidence(
	callers: &[crate::storage_port::AgentCallerRow],
) -> CallersSummaryEvidence {
	let count = callers.len() as u64;
	let top_modules = group_by_module(
		callers.iter().map(|c| c.module_path.as_deref()),
	);
	CallersSummaryEvidence { count, top_modules }
}

fn build_callees_evidence(
	callees: &[crate::storage_port::AgentCalleeRow],
) -> CalleesSummaryEvidence {
	let count = callees.len() as u64;
	let top_modules = group_by_module(
		callees.iter().map(|c| c.module_path.as_deref()),
	);
	CalleesSummaryEvidence { count, top_modules }
}

/// Group items by module_path, count occurrences, sort descending
/// by count (tiebreak by module name ascending), truncate to
/// TOP_MODULES_N.
fn group_by_module<'a>(
	module_paths: impl Iterator<Item = Option<&'a str>>,
) -> Vec<ModuleCountEvidence> {
	let mut counts: HashMap<String, u64> = HashMap::new();
	for mp in module_paths {
		let key = mp.unwrap_or("(unknown)").to_string();
		*counts.entry(key).or_insert(0) += 1;
	}
	let mut entries: Vec<ModuleCountEvidence> = counts
		.into_iter()
		.map(|(module, count)| ModuleCountEvidence { module, count })
		.collect();
	entries.sort_by(|a, b| {
		b.count
			.cmp(&a.count)
			.then_with(|| a.module.cmp(&b.module))
	});
	entries.truncate(TOP_MODULES_N);
	entries
}

// ── Boundary aggregator for exact module match ──────────────────

use crate::dto::signal::{
	BoundaryViolationEvidence, BoundaryViolationsEvidence,
};
use crate::errors::AgentStorageError;

fn aggregate_boundary_for_module<S: AgentStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	snapshot_uid: &str,
	module_path: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
	let declarations = storage.get_active_boundary_declarations(repo_uid)?;

	// Filter to declarations where source_module == module_path
	// EXACT match, not prefix.
	let matching: Vec<_> = declarations
		.into_iter()
		.filter(|d| d.source_module == module_path)
		.collect();

	if matching.is_empty() {
		return Ok(AggregatorOutput::empty());
	}

	let mut per_rule: Vec<BoundaryViolationEvidence> = Vec::new();
	let mut total_edges: u64 = 0;

	for decl in matching {
		let edges = storage.find_imports_between_paths(
			snapshot_uid,
			&decl.source_module,
			&decl.forbidden_target,
		)?;
		if edges.is_empty() {
			continue;
		}
		let edge_count = edges.len() as u64;
		total_edges += edge_count;
		per_rule.push(BoundaryViolationEvidence {
			source_module: decl.source_module,
			target_module: decl.forbidden_target,
			edge_count,
		});
	}

	if total_edges == 0 {
		return Ok(AggregatorOutput::empty());
	}

	per_rule.sort_by(|a, b| {
		b.edge_count
			.cmp(&a.edge_count)
			.then_with(|| a.source_module.cmp(&b.source_module))
			.then_with(|| a.target_module.cmp(&b.target_module))
	});
	per_rule.truncate(3);

	let evidence = BoundaryViolationsEvidence {
		violation_count: total_edges,
		top_violations: per_rule,
	};

	Ok(AggregatorOutput {
		signals: vec![Signal::boundary_violations(evidence)],
		limits: Vec::new(),
	})
}

// ── Cycle aggregator for exact module match ─────────────────────

use crate::dto::signal::{CycleEvidence, ImportCyclesEvidence};

fn aggregate_cycles_for_module<S: AgentStorageRead + ?Sized>(
	storage: &S,
	snapshot_uid: &str,
	module_qualified_name: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
	let cycles =
		storage.find_cycles_involving_module(snapshot_uid, module_qualified_name)?;

	if cycles.is_empty() {
		return Ok(AggregatorOutput::empty());
	}

	let cycle_count = cycles.len() as u64;
	let top: Vec<CycleEvidence> = cycles
		.into_iter()
		.take(3)
		.map(|c| CycleEvidence {
			length: c.length,
			modules: c.modules,
		})
		.collect();

	let evidence = ImportCyclesEvidence { cycle_count, cycles: top };

	Ok(AggregatorOutput {
		signals: vec![Signal::import_cycles(evidence)],
		limits: Vec::new(),
	})
}

// ── Gate aggregator for exact module match ──────────────────────

use repo_graph_gate::{
	assemble_from_requirements, GateMode, GateRequirement,
};

fn aggregate_gate_for_module<S: GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	snapshot_uid: &str,
	now: &str,
	module_path: &str,
) -> Result<AggregatorOutput, OrientError> {
	let requirements = storage
		.get_active_requirements(repo_uid)
		.map_err(|e| {
			OrientError::Storage(crate::errors::AgentStorageError::new(
				"get_active_requirements",
				e.message,
			))
		})?;

	if requirements.is_empty() {
		return Ok(AggregatorOutput {
			signals: Vec::new(),
			limits: vec![Limit::from_code(LimitCode::GateNotConfigured)],
		});
	}

	// Filter obligations by EXACT target match (not prefix).
	let filtered: Vec<GateRequirement> = requirements
		.into_iter()
		.filter_map(|req| {
			let matching: Vec<_> = req
				.obligations
				.into_iter()
				.filter(|o| match &o.target {
					Some(t) => t == module_path,
					None => false,
				})
				.collect();
			if matching.is_empty() {
				None
			} else {
				Some(GateRequirement {
					req_id: req.req_id,
					version: req.version,
					obligations: matching,
				})
			}
		})
		.collect();

	if filtered.is_empty() {
		return Ok(AggregatorOutput {
			signals: Vec::new(),
			limits: vec![Limit::from_code(
				LimitCode::GateNotApplicableToFocus,
			)],
		});
	}

	let report = match assemble_from_requirements(
		storage,
		repo_uid,
		snapshot_uid,
		GateMode::Default,
		now,
		filtered,
	) {
		Ok(r) => r,
		Err(e) => return Err(map_gate_error(e)),
	};

	let signals = project_report(&report);

	Ok(AggregatorOutput { signals, limits: Vec::new() })
}

fn project_report(
	report: &repo_graph_gate::GateReport,
) -> Vec<Signal> {
	use crate::dto::signal::{
		GateFailEvidence, GateIncompleteEvidence, GatePassEvidence,
	};

	if report.outcome.counts.total == 0 {
		return Vec::new();
	}

	let counts = &report.outcome.counts;

	match report.outcome.outcome.as_str() {
		"fail" => {
			let failing_obligations: Vec<String> = report
				.obligations
				.iter()
				.filter(|o| {
					matches!(
						o.effective_verdict,
						repo_graph_gate::EffectiveVerdict::FAIL
					)
				})
				.map(|o| format!("{}/{}", o.req_id, o.obligation_id))
				.collect();

			vec![Signal::gate_fail(GateFailEvidence {
				fail_count: counts.fail as u64,
				total_count: counts.total as u64,
				failing_obligations,
			})]
		}
		"incomplete" => {
			vec![Signal::gate_incomplete(
				GateIncompleteEvidence {
					missing_count: counts.missing_evidence as u64,
					unsupported_count: counts.unsupported as u64,
					total_count: counts.total as u64,
				},
			)]
		}
		"pass" => vec![Signal::gate_pass(GatePassEvidence {
			pass_count: counts.pass as u64,
			waived_count: counts.waived as u64,
			total_count: counts.total as u64,
		})],
		_ => Vec::new(),
	}
}

fn map_gate_error(e: repo_graph_gate::GateError) -> OrientError {
	match e {
		repo_graph_gate::GateError::Storage(inner) => {
			OrientError::Storage(crate::errors::AgentStorageError::new(
				inner.operation,
				inner.message,
			))
		}
		repo_graph_gate::GateError::MalformedEvidence {
			operation,
			reason,
		} => OrientError::Storage(
			crate::errors::AgentStorageError::new(operation, reason),
		),
	}
}

fn merge(
	signals: &mut Vec<Signal>,
	limits: &mut Vec<Limit>,
	out: AggregatorOutput,
) {
	signals.extend(out.signals);
	limits.extend(out.limits);
}

/// Build the documentation section for symbol-scoped orient.
///
/// Uses the symbol's file path for doc relevance selection (same
/// semantics as file focus). Returns None if the symbol has no
/// file association or no relevant docs exist.
fn build_documentation_section<S: AgentStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	file_path: Option<&str>,
) -> Option<DocumentationSection> {
	let file_path = file_path?;

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

	let focus = DocFocusContext::symbol(Some(file_path));
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
