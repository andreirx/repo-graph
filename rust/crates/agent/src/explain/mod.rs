//! Explain use case — multi-section detail pipeline.
//!
//! `run_explain` resolves a target to a symbol, file, or path area
//! and emits typed explain sections covering identity, callers,
//! callees, imports, symbols, files, dead code, cycles, boundary
//! violations, gate, trust, and measurements.
//!
//! Focus resolution reuses `orient`'s resolution logic so the same
//! target string resolves to the same entity in both commands.

use std::collections::HashMap;

use repo_graph_gate::GateStorageRead;

use crate::confidence::derive_repo_confidence;
use crate::dto::budget::Budget;
use crate::dto::envelope::{
	Confidence, Focus, OrientResult, EXPLAIN_COMMAND, ORIENT_SCHEMA,
};
use crate::dto::signal::*;
use crate::errors::{AgentStorageError, ExplainError};
use crate::ranking;
use crate::storage_port::{
	AgentReliabilityLevel, AgentSnapshot, AgentStorageRead,
	AgentSymbolContext,
};

/// Items cap per budget tier (medium minimum, large optional).
fn items_cap(budget: Budget) -> usize {
	match budget {
		Budget::Small | Budget::Medium => 15,
		Budget::Large => 50,
	}
}

/// Truncate a list to the items cap and return truncation metadata.
fn truncate_items<T>(items: &mut Vec<T>, cap: usize) -> (Option<bool>, Option<u64>) {
	if items.len() <= cap {
		(None, None)
	} else {
		let omitted = items.len() - cap;
		items.truncate(cap);
		(Some(true), Some(omitted as u64))
	}
}

/// Entry point for the explain use case.
pub fn run_explain<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	target: &str,
	budget: Budget,
	now: &str,
) -> Result<OrientResult, ExplainError> {
	// Budget: minimum medium.
	let budget = match budget {
		Budget::Small => Budget::Medium,
		other => other,
	};

	// ── 1. Resolve repo identity. ────────────────────────────
	let repo = storage
		.get_repo(repo_uid)?
		.ok_or_else(|| ExplainError::NoRepo { repo_uid: repo_uid.to_string() })?;

	// ── 2. Resolve snapshot. ─────────────────────────────────
	let snapshot = storage
		.get_latest_snapshot(repo_uid)?
		.ok_or_else(|| ExplainError::NoSnapshot {
			repo_uid: repo_uid.to_string(),
		})?;

	let snapshot_uid = &snapshot.snapshot_uid;

	// ── 3. Resolve focus (reusing orient's resolution logic). ─
	let resolution = storage.resolve_path_focus(snapshot_uid, target)?;

	if resolution.has_exact_file {
		return explain_file(
			storage, &repo.name, &snapshot, target,
			resolution.file_stable_key.as_deref(), budget, now,
		);
	}

	if resolution.has_content_under_prefix
		|| resolution.module_stable_key.is_some()
	{
		return explain_path(
			storage, &repo.name, &snapshot, target,
			resolution.module_stable_key.as_deref(), budget, now,
		);
	}

	// ── 4. Try stable-key resolution. ────────────────────────
	use crate::storage_port::AgentFocusKind;

	match storage.resolve_stable_key_focus(snapshot_uid, target)? {
		Some(candidate) if candidate.kind == AgentFocusKind::Symbol => {
			let context =
				storage.get_symbol_context(snapshot_uid, &candidate.stable_key)?;
			match context {
				Some(ctx) => explain_symbol(
					storage, &repo.name, &snapshot,
					&candidate.stable_key, &ctx, target, budget, now,
				),
				None => Ok(build_no_match(
					&repo.name, &snapshot, target, budget,
				)),
			}
		}
		Some(candidate) if candidate.kind == AgentFocusKind::File => {
			let file_path = candidate.file.as_deref().unwrap_or(target);
			explain_file(
				storage, &repo.name, &snapshot, file_path,
				Some(&candidate.stable_key), budget, now,
			)
		}
		Some(candidate) => {
			// MODULE by stable key.
			let path = extract_path_from_candidate(&candidate, target);
			explain_path(
				storage, &repo.name, &snapshot, &path,
				Some(&candidate.stable_key), budget, now,
			)
		}
		None => {
			// ── 5. Try symbol name resolution. ──────────────
			let symbol_candidates =
				storage.resolve_symbol_name(snapshot_uid, target)?;
			match symbol_candidates.len() {
				0 => Ok(build_no_match(
					&repo.name, &snapshot, target, budget,
				)),
				1 => {
					let candidate = &symbol_candidates[0];
					let context = storage.get_symbol_context(
						snapshot_uid,
						&candidate.stable_key,
					)?;
					match context {
						Some(ctx) => explain_symbol(
							storage, &repo.name, &snapshot,
							&candidate.stable_key, &ctx, target,
							budget, now,
						),
						None => Ok(build_no_match(
							&repo.name, &snapshot, target, budget,
						)),
					}
				}
				_ => {
					// Ambiguous — return candidates.
					let focus_candidates = symbol_candidates
						.into_iter()
						.map(|c| crate::dto::envelope::FocusCandidate {
							stable_key: c.stable_key,
							file: c.file,
							kind: crate::dto::envelope::ResolvedKind::Symbol,
						})
						.collect();
					Ok(OrientResult {
						schema: ORIENT_SCHEMA,
						command: EXPLAIN_COMMAND,
						repo: repo.name,
						snapshot: snapshot.snapshot_uid.clone(),
						focus: Focus::ambiguous(target, focus_candidates),
						confidence: Confidence::High,
						signals: Vec::new(),
						signals_truncated: None,
						signals_omitted_count: None,
						limits: Vec::new(),
						limits_truncated: None,
						limits_omitted_count: None,
						next: Vec::new(),
						next_truncated: None,
						next_omitted_count: None,
						truncated: false,
					})
				}
			}
		}
	}
}

fn extract_path_from_candidate(
	candidate: &crate::storage_port::AgentFocusCandidate,
	focus_str: &str,
) -> String {
	if let Some(ref f) = candidate.file {
		return f.clone();
	}
	let key = &candidate.stable_key;
	if let Some(stripped) = key.strip_suffix(":MODULE") {
		if let Some(colon) = stripped.find(':') {
			return stripped[colon + 1..].to_string();
		}
	}
	focus_str.to_string()
}

fn build_no_match(
	repo_name: &str,
	snapshot: &AgentSnapshot,
	target: &str,
	_budget: Budget,
) -> OrientResult {
	OrientResult {
		schema: ORIENT_SCHEMA,
		command: EXPLAIN_COMMAND,
		repo: repo_name.to_string(),
		snapshot: snapshot.snapshot_uid.clone(),
		focus: Focus::no_match(target),
		confidence: Confidence::High,
		signals: Vec::new(),
		signals_truncated: None,
		signals_omitted_count: None,
		limits: Vec::new(),
		limits_truncated: None,
		limits_omitted_count: None,
		next: Vec::new(),
		next_truncated: None,
		next_omitted_count: None,
		truncated: false,
	}
}

// ── Symbol explain pipeline ─────────────────────────────────────────

fn explain_symbol<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_name: &str,
	snapshot: &AgentSnapshot,
	symbol_stable_key: &str,
	context: &AgentSymbolContext,
	focus_input: &str,
	budget: Budget,
	now: &str,
) -> Result<OrientResult, ExplainError> {
	let snapshot_uid = &snapshot.snapshot_uid;
	let repo_uid = &snapshot.repo_uid;
	let cap = items_cap(budget);
	let mut signals: Vec<Signal> = Vec::new();

	// ── EXPLAIN_IDENTITY ────────────────────────────────────
	signals.push(Signal::explain_identity(ExplainIdentityEvidence {
		target_kind: "symbol".to_string(),
		path: context.file_path.clone(),
		stable_key: Some(symbol_stable_key.to_string()),
		name: Some(context.name.clone()),
		subtype: context.subtype.clone(),
		line_start: context.line_start,
		language: None,
		is_test: None,
		module_path: context.module_path.clone(),
		file_count: None,
		symbol_count: None,
	}));

	// ── EXPLAIN_CALLERS ─────────────────────────────────────
	// Always emitted for symbol targets — "0 callers" is
	// meaningful positive information in a deep dive.
	let callers = storage.find_symbol_callers(snapshot_uid, symbol_stable_key)?;
	{
		let count = callers.len() as u64;
		let top_modules = group_by_module(
			callers.iter().map(|c| c.module_path.as_deref()),
		);
		let mut items: Vec<ExplainCallerItem> = callers
			.iter()
			.map(|c| ExplainCallerItem {
				stable_key: c.stable_key.clone(),
				name: c.name.clone(),
				module: c.module_path.clone(),
			})
			.collect();
		let (trunc, omitted) = truncate_items(&mut items, cap);
		signals.push(Signal::explain_callers(ExplainCallersEvidence {
			count,
			top_modules,
			items,
			items_truncated: trunc,
			items_omitted_count: omitted,
		}));
	}

	// ── EXPLAIN_CALLEES ─────────────────────────────────────
	// Always emitted for symbol targets — same reasoning.
	let callees = storage.find_symbol_callees(snapshot_uid, symbol_stable_key)?;
	{
		let count = callees.len() as u64;
		let top_modules = group_by_module(
			callees.iter().map(|c| c.module_path.as_deref()),
		);
		let mut items: Vec<ExplainCalleeItem> = callees
			.iter()
			.map(|c| ExplainCalleeItem {
				stable_key: c.stable_key.clone(),
				name: c.name.clone(),
				module: c.module_path.clone(),
			})
			.collect();
		let (trunc, omitted) = truncate_items(&mut items, cap);
		signals.push(Signal::explain_callees(ExplainCalleesEvidence {
			count,
			top_modules,
			items,
			items_truncated: trunc,
			items_omitted_count: omitted,
		}));
	}

	// ── EXPLAIN_DEAD (symbol-scoped) ────────────────────────
	let trust = storage.get_trust_summary(repo_uid, snapshot_uid)?;
	let reliability_level = match trust.dead_code_reliability.level {
		AgentReliabilityLevel::High => "high",
		AgentReliabilityLevel::Medium => "medium",
		AgentReliabilityLevel::Low => "low",
	}
	.to_string();

	if let Some(ref file_path) = context.file_path {
		let dead = storage.find_dead_nodes_in_file(
			snapshot_uid, repo_uid, file_path,
		)?;
		let is_target_dead =
			dead.iter().any(|d| d.stable_key == symbol_stable_key);
		let dead_count = if is_target_dead { 1 } else { 0 };
		signals.push(Signal::explain_dead(ExplainDeadEvidence {
			count: dead_count,
			is_target_dead: Some(is_target_dead),
			reliability_level: reliability_level.clone(),
			reliability_reasons: trust.dead_code_reliability.reasons.clone(),
			items: if is_target_dead {
				dead.iter()
					.filter(|d| d.stable_key == symbol_stable_key)
					.map(|d| ExplainDeadItem {
						symbol: d.symbol.clone(),
						file: d.file.clone(),
						line_count: d.line_count,
					})
					.collect()
			} else {
				Vec::new()
			},
			items_truncated: None,
			items_omitted_count: None,
		}));
	}

	// ── Inherited module-context signals ────────────────────
	if let Some(ref module_path) = context.module_path {
		// EXPLAIN_CYCLES
		let cycles = storage.find_cycles_involving_module(
			snapshot_uid, module_path,
		)?;
		if !cycles.is_empty() {
			let count = cycles.len() as u64;
			let mut items: Vec<CycleEvidence> = cycles
				.into_iter()
				.map(|c| CycleEvidence {
					length: c.length,
					modules: c.modules,
				})
				.collect();
			let (trunc, omitted) = truncate_items(&mut items, cap);
			signals.push(
				Signal::explain_cycles(ExplainCyclesEvidence {
					count,
					items,
					items_truncated: trunc,
					items_omitted_count: omitted,
				})
				.with_module_context(),
			);
		}

		// EXPLAIN_BOUNDARY
		let declarations = storage.get_active_boundary_declarations(repo_uid)?;
		let matching: Vec<_> = declarations
			.into_iter()
			.filter(|d| d.source_module == *module_path)
			.collect();
		if !matching.is_empty() {
			let mut per_rule: Vec<BoundaryViolationEvidence> = Vec::new();
			let mut total = 0u64;
			for decl in matching {
				let edges = storage.find_imports_between_paths(
					snapshot_uid, &decl.source_module, &decl.forbidden_target,
				)?;
				if edges.is_empty() {
					continue;
				}
				let edge_count = edges.len() as u64;
				total += edge_count;
				per_rule.push(BoundaryViolationEvidence {
					source_module: decl.source_module,
					target_module: decl.forbidden_target,
					edge_count,
				});
			}
			if total > 0 {
				let (trunc, omitted) = truncate_items(&mut per_rule, cap);
				signals.push(
					Signal::explain_boundary(ExplainBoundaryEvidence {
						violation_count: total,
						items: per_rule,
						items_truncated: trunc,
						items_omitted_count: omitted,
					})
					.with_module_context(),
				);
			}
		}

		// EXPLAIN_GATE
		build_gate_signal(
			storage, repo_uid, snapshot_uid, now, Some(module_path),
			cap, &mut signals, true,
		)?;
	}

	// ── EXPLAIN_TRUST ───────────────────────────────────────
	signals.push(build_trust_signal(&trust));

	// ── EXPLAIN_MEASUREMENTS ────────────────────────────────
	// Omit when no measurement items exist. The Rust indexer
	// does not currently produce measurements; this section
	// activates when coverage or complexity data is present.
	let measurement_items: Vec<ExplainMeasurementItem> = Vec::new();
	if !measurement_items.is_empty() {
		signals.push(Signal::explain_measurements(ExplainMeasurementsEvidence {
			items: measurement_items,
			items_truncated: None,
			items_omitted_count: None,
		}));
	}

	// ── ranking + truncation ────────────────────────────────
	ranking::sort_and_rank(&mut signals);
	let sig_tx = ranking::truncate_signals(&mut signals, budget);

	let stale = !storage.get_stale_files(snapshot_uid)?.is_empty();
	let confidence = derive_repo_confidence(&trust, stale);

	let focus = Focus::symbol(
		focus_input,
		symbol_stable_key,
		context.file_path.as_deref(),
	);

	Ok(OrientResult {
		schema: ORIENT_SCHEMA,
		command: EXPLAIN_COMMAND,
		repo: repo_name.to_string(),
		snapshot: snapshot_uid.clone(),
		focus,
		confidence,
		signals,
		signals_truncated: sig_tx.truncated.then_some(true),
		signals_omitted_count: sig_tx.truncated.then_some(sig_tx.omitted),
		limits: Vec::new(),
		limits_truncated: None,
		limits_omitted_count: None,
		next: Vec::new(),
		next_truncated: None,
		next_omitted_count: None,
		truncated: sig_tx.truncated,
	})
}

// ── File explain pipeline ───────────────────────────────────────────

fn explain_file<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_name: &str,
	snapshot: &AgentSnapshot,
	file_path: &str,
	file_stable_key: Option<&str>,
	budget: Budget,
	now: &str,
) -> Result<OrientResult, ExplainError> {
	let _ = now;
	let snapshot_uid = &snapshot.snapshot_uid;
	let repo_uid = &snapshot.repo_uid;
	let cap = items_cap(budget);
	let mut signals: Vec<Signal> = Vec::new();

	// ── EXPLAIN_IDENTITY ────────────────────────────────────
	let file_summary = storage.compute_file_summary(snapshot_uid, file_path)?;
	signals.push(Signal::explain_identity(ExplainIdentityEvidence {
		target_kind: "file".to_string(),
		path: Some(file_path.to_string()),
		stable_key: file_stable_key.map(|k| k.to_string()),
		name: None,
		subtype: None,
		line_start: None,
		language: file_summary.languages.first().cloned(),
		is_test: None,
		module_path: None,
		file_count: None,
		symbol_count: Some(file_summary.symbol_count),
	}));

	// ── EXPLAIN_IMPORTS ─────────────────────────────────────
	let imports = storage.find_file_imports(snapshot_uid, file_path)?;
	if !imports.is_empty() {
		let count = imports.len() as u64;
		let mut items: Vec<ExplainImportItem> = imports
			.into_iter()
			.map(|i| ExplainImportItem {
				target_file: i.target_file,
			})
			.collect();
		let (trunc, omitted) = truncate_items(&mut items, cap);
		signals.push(Signal::explain_imports(ExplainImportsEvidence {
			count,
			items,
			items_truncated: trunc,
			items_omitted_count: omitted,
		}));
	}

	// ── EXPLAIN_SYMBOLS ─────────────────────────────────────
	let trust = storage.get_trust_summary(repo_uid, snapshot_uid)?;
	let symbols = storage.list_symbols_in_file(snapshot_uid, file_path)?;
	let dead_nodes = storage.find_dead_nodes_in_file(
		snapshot_uid, repo_uid, file_path,
	)?;
	let dead_keys: std::collections::HashSet<&str> = dead_nodes
		.iter()
		.map(|d| d.stable_key.as_str())
		.collect();

	if !symbols.is_empty() {
		let count = symbols.len() as u64;
		let mut items: Vec<ExplainSymbolItem> = symbols
			.iter()
			.map(|s| ExplainSymbolItem {
				name: s.name.clone(),
				subtype: s.subtype.clone(),
				line_start: s.line_start,
				is_dead: dead_keys.contains(s.stable_key.as_str()),
			})
			.collect();
		let (trunc, omitted) = truncate_items(&mut items, cap);
		signals.push(Signal::explain_symbols(ExplainSymbolsEvidence {
			count,
			items,
			items_truncated: trunc,
			items_omitted_count: omitted,
		}));
	}

	// ── EXPLAIN_DEAD (file-scoped) ──────────────────────────
	let reliability_level = match trust.dead_code_reliability.level {
		AgentReliabilityLevel::High => "high",
		AgentReliabilityLevel::Medium => "medium",
		AgentReliabilityLevel::Low => "low",
	}
	.to_string();

	{
		let count = dead_nodes.len() as u64;
		let mut items: Vec<ExplainDeadItem> = dead_nodes
			.iter()
			.map(|d| ExplainDeadItem {
				symbol: d.symbol.clone(),
				file: d.file.clone(),
				line_count: d.line_count,
			})
			.collect();
		let (trunc, omitted) = truncate_items(&mut items, cap);
		signals.push(Signal::explain_dead(ExplainDeadEvidence {
			count,
			is_target_dead: None,
			reliability_level,
			reliability_reasons: trust.dead_code_reliability.reasons.clone(),
			items,
			items_truncated: trunc,
			items_omitted_count: omitted,
		}));
	}

	// ── EXPLAIN_TRUST ───────────────────────────────────────
	signals.push(build_trust_signal(&trust));

	// ── EXPLAIN_MEASUREMENTS ────────────────────────────────
	// Omit when no measurement items exist. The Rust indexer
	// does not currently produce measurements; this section
	// activates when coverage or complexity data is present.
	let measurement_items: Vec<ExplainMeasurementItem> = Vec::new();
	if !measurement_items.is_empty() {
		signals.push(Signal::explain_measurements(ExplainMeasurementsEvidence {
			items: measurement_items,
			items_truncated: None,
			items_omitted_count: None,
		}));
	}

	// ── ranking + truncation ────────────────────────────────
	ranking::sort_and_rank(&mut signals);
	let sig_tx = ranking::truncate_signals(&mut signals, budget);

	let stale = !storage.get_stale_files(snapshot_uid)?.is_empty();
	let confidence = derive_repo_confidence(&trust, stale);

	let focus = Focus::file(file_path, file_stable_key, file_path);

	Ok(OrientResult {
		schema: ORIENT_SCHEMA,
		command: EXPLAIN_COMMAND,
		repo: repo_name.to_string(),
		snapshot: snapshot_uid.clone(),
		focus,
		confidence,
		signals,
		signals_truncated: sig_tx.truncated.then_some(true),
		signals_omitted_count: sig_tx.truncated.then_some(sig_tx.omitted),
		limits: Vec::new(),
		limits_truncated: None,
		limits_omitted_count: None,
		next: Vec::new(),
		next_truncated: None,
		next_omitted_count: None,
		truncated: sig_tx.truncated,
	})
}

// ── Path explain pipeline ───────────────────────────────────────────

fn explain_path<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_name: &str,
	snapshot: &AgentSnapshot,
	path_prefix: &str,
	module_stable_key: Option<&str>,
	budget: Budget,
	now: &str,
) -> Result<OrientResult, ExplainError> {
	let snapshot_uid = &snapshot.snapshot_uid;
	let repo_uid = &snapshot.repo_uid;
	let cap = items_cap(budget);
	let mut signals: Vec<Signal> = Vec::new();

	// ── EXPLAIN_IDENTITY ────────────────────────────────────
	let path_summary = storage.compute_path_summary(snapshot_uid, path_prefix)?;
	signals.push(Signal::explain_identity(ExplainIdentityEvidence {
		target_kind: "path".to_string(),
		path: Some(path_prefix.to_string()),
		stable_key: module_stable_key.map(|k| k.to_string()),
		name: None,
		subtype: None,
		line_start: None,
		language: None,
		is_test: None,
		module_path: Some(path_prefix.to_string()),
		file_count: Some(path_summary.file_count),
		symbol_count: Some(path_summary.symbol_count),
	}));

	// ── EXPLAIN_FILES ───────────────────────────────────────
	let files = storage.list_files_in_path(snapshot_uid, path_prefix)?;
	if !files.is_empty() {
		let count = files.len() as u64;
		let mut items: Vec<ExplainFileItem> = files
			.into_iter()
			.map(|f| ExplainFileItem {
				path: f.path,
				symbol_count: f.symbol_count,
				is_test: f.is_test,
			})
			.collect();
		let (trunc, omitted) = truncate_items(&mut items, cap);
		signals.push(Signal::explain_files(ExplainFilesEvidence {
			count,
			items,
			items_truncated: trunc,
			items_omitted_count: omitted,
		}));
	}

	// ── EXPLAIN_DEAD (path-scoped) ──────────────────────────
	let trust = storage.get_trust_summary(repo_uid, snapshot_uid)?;
	let reliability_level = match trust.dead_code_reliability.level {
		AgentReliabilityLevel::High => "high",
		AgentReliabilityLevel::Medium => "medium",
		AgentReliabilityLevel::Low => "low",
	}
	.to_string();

	let dead = storage.find_dead_nodes_in_path(
		snapshot_uid, repo_uid, path_prefix,
	)?;
	{
		let count = dead.len() as u64;
		let mut items: Vec<ExplainDeadItem> = dead
			.iter()
			.map(|d| ExplainDeadItem {
				symbol: d.symbol.clone(),
				file: d.file.clone(),
				line_count: d.line_count,
			})
			.collect();
		let (trunc, omitted) = truncate_items(&mut items, cap);
		signals.push(Signal::explain_dead(ExplainDeadEvidence {
			count,
			is_target_dead: None,
			reliability_level,
			reliability_reasons: trust.dead_code_reliability.reasons.clone(),
			items,
			items_truncated: trunc,
			items_omitted_count: omitted,
		}));
	}

	// ── EXPLAIN_CYCLES ──────────────────────────────────────
	let cycles = storage.find_cycles_involving_path(
		snapshot_uid, path_prefix,
	)?;
	if !cycles.is_empty() {
		let count = cycles.len() as u64;
		let mut items: Vec<CycleEvidence> = cycles
			.into_iter()
			.map(|c| CycleEvidence {
				length: c.length,
				modules: c.modules,
			})
			.collect();
		let (trunc, omitted) = truncate_items(&mut items, cap);
		signals.push(Signal::explain_cycles(ExplainCyclesEvidence {
			count,
			items,
			items_truncated: trunc,
			items_omitted_count: omitted,
		}));
	}

	// ── EXPLAIN_BOUNDARY ────────────────────────────────────
	let declarations = storage.find_boundary_declarations_in_path(
		repo_uid, path_prefix,
	)?;
	if !declarations.is_empty() {
		let mut per_rule: Vec<BoundaryViolationEvidence> = Vec::new();
		let mut total = 0u64;
		for decl in declarations {
			let edges = storage.find_imports_between_paths(
				snapshot_uid, &decl.source_module, &decl.forbidden_target,
			)?;
			if edges.is_empty() {
				continue;
			}
			let edge_count = edges.len() as u64;
			total += edge_count;
			per_rule.push(BoundaryViolationEvidence {
				source_module: decl.source_module,
				target_module: decl.forbidden_target,
				edge_count,
			});
		}
		if total > 0 {
			let (trunc, omitted) = truncate_items(&mut per_rule, cap);
			signals.push(Signal::explain_boundary(ExplainBoundaryEvidence {
				violation_count: total,
				items: per_rule,
				items_truncated: trunc,
				items_omitted_count: omitted,
			}));
		}
	}

	// ── EXPLAIN_GATE ────────────────────────────────────────
	build_gate_signal(
		storage, repo_uid, snapshot_uid, now, Some(path_prefix),
		cap, &mut signals, false,
	)?;

	// ── EXPLAIN_TRUST ───────────────────────────────────────
	signals.push(build_trust_signal(&trust));

	// ── EXPLAIN_MEASUREMENTS ────────────────────────────────
	// Omit when no measurement items exist. The Rust indexer
	// does not currently produce measurements; this section
	// activates when coverage or complexity data is present.
	let measurement_items: Vec<ExplainMeasurementItem> = Vec::new();
	if !measurement_items.is_empty() {
		signals.push(Signal::explain_measurements(ExplainMeasurementsEvidence {
			items: measurement_items,
			items_truncated: None,
			items_omitted_count: None,
		}));
	}

	// ── ranking + truncation ────────────────────────────────
	ranking::sort_and_rank(&mut signals);
	let sig_tx = ranking::truncate_signals(&mut signals, budget);

	let stale = !storage.get_stale_files(snapshot_uid)?.is_empty();
	let confidence = derive_repo_confidence(&trust, stale);

	let focus = Focus::path_area(path_prefix, module_stable_key, path_prefix);

	Ok(OrientResult {
		schema: ORIENT_SCHEMA,
		command: EXPLAIN_COMMAND,
		repo: repo_name.to_string(),
		snapshot: snapshot_uid.clone(),
		focus,
		confidence,
		signals,
		signals_truncated: sig_tx.truncated.then_some(true),
		signals_omitted_count: sig_tx.truncated.then_some(sig_tx.omitted),
		limits: Vec::new(),
		limits_truncated: None,
		limits_omitted_count: None,
		next: Vec::new(),
		next_truncated: None,
		next_omitted_count: None,
		truncated: sig_tx.truncated,
	})
}

// ── Shared helpers ──────────────────────────────────────────────────

const TOP_MODULES_N: usize = 3;

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

fn build_trust_signal(
	trust: &crate::storage_port::AgentTrustSummary,
) -> Signal {
	use crate::storage_port::EnrichmentState;
	Signal::explain_trust(ExplainTrustEvidence {
		call_resolution_rate: trust.call_resolution_rate,
		call_graph_reliability: match trust.call_graph_reliability.level {
			AgentReliabilityLevel::High => "high".to_string(),
			AgentReliabilityLevel::Medium => "medium".to_string(),
			AgentReliabilityLevel::Low => "low".to_string(),
		},
		dead_code_reliability: match trust.dead_code_reliability.level {
			AgentReliabilityLevel::High => "high".to_string(),
			AgentReliabilityLevel::Medium => "medium".to_string(),
			AgentReliabilityLevel::Low => "low".to_string(),
		},
		enrichment_state: match trust.enrichment_state {
			EnrichmentState::Ran => "ran".to_string(),
			EnrichmentState::NotApplicable => "not_applicable".to_string(),
			EnrichmentState::NotRun => "not_run".to_string(),
		},
	})
}

fn build_gate_signal<S: GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	snapshot_uid: &str,
	now: &str,
	target_filter: Option<&str>,
	cap: usize,
	signals: &mut Vec<Signal>,
	module_context: bool,
) -> Result<(), ExplainError> {
	use repo_graph_gate::{
		assemble_from_requirements, GateMode, GateRequirement,
	};

	let requirements = storage
		.get_active_requirements(repo_uid)
		.map_err(|e| {
			AgentStorageError::new("get_active_requirements", e.message)
		})?;

	if requirements.is_empty() {
		return Ok(());
	}

	// Filter obligations by target.
	let filtered: Vec<GateRequirement> = if let Some(target) = target_filter {
		requirements
			.into_iter()
			.filter_map(|req| {
				let matching: Vec<_> = req
					.obligations
					.into_iter()
					.filter(|o| match &o.target {
						Some(t) => {
							if module_context {
								t == target
							} else {
								t == target
									|| t.starts_with(&format!("{}/", target))
							}
						}
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
			.collect()
	} else {
		requirements
	};

	if filtered.is_empty() {
		return Ok(());
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
		Err(_) => return Ok(()),
	};

	if report.outcome.counts.total == 0 {
		return Ok(());
	}

	let obligation_count = report.outcome.counts.total as u64;
	let outcome = report.outcome.outcome.clone();
	let mut items: Vec<ExplainGateItem> = report
		.obligations
		.iter()
		.map(|o| ExplainGateItem {
			req_id: o.req_id.clone(),
			obligation_id: o.obligation_id.clone(),
			method: o.method.clone(),
			effective_verdict: format!("{:?}", o.effective_verdict),
		})
		.collect();
	let (trunc, omitted) = truncate_items(&mut items, cap);

	let sig = Signal::explain_gate(ExplainGateEvidence {
		outcome,
		obligation_count,
		items,
		items_truncated: trunc,
		items_omitted_count: omitted,
	});

	if module_context {
		signals.push(sig.with_module_context());
	} else {
		signals.push(sig);
	}

	Ok(())
}
