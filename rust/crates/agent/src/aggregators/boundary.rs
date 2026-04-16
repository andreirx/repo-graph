//! Boundary-violation aggregator.
//!
//! For each active boundary declaration `A -> forbids B`, the
//! aggregator asks storage for IMPORTS edges from any file under
//! path `A` to any file under path `B`. Any such edge is a
//! violation.
//!
//! Emission rule: emit `BOUNDARY_VIOLATIONS` signal iff at least
//! one violation is found across all declarations. Evidence
//! carries the total edge count plus up to 3 `(source_module,
//! target_module, edge_count)` aggregates as "top violations".
//!
//! ── Declaration deduplication ────────────────────────────────
//!
//! Active declarations are deduplicated by
//! `(source_module, forbidden_target)` BEFORE the storage is
//! queried for violating edges. This matches the existing
//! `rmap violations` CLI command behavior (see
//! `rust/crates/rgr/src/main.rs` `run_violations`) and prevents
//! double-counting when legacy TS authoring, raw SQL seeding, or
//! data migrations produce multiple active declarations that
//! name the same logical rule.
//!
//! First-seen wins for the `reason` field, also matching the
//! `violations` command semantics. Iteration order of the
//! original declaration list determines which reason survives;
//! the storage port already guarantees that order is stable
//! (`ORDER BY created_at DESC` at the SQL level).
//!
//! IMPORTANT: gate semantics are NOT applied here. This is
//! raw boundary-declaration enforcement, not obligation
//! evaluation. Gate evaluation remains blocked on the
//! `gate.rs` relocation tech-debt item; see the `gate.rs`
//! comment in `orient/repo.rs`.

use std::collections::HashSet;

use super::AggregatorOutput;
use crate::dto::signal::{
	BoundaryViolationEvidence, BoundaryViolationsEvidence, Signal,
};
use crate::errors::AgentStorageError;
use crate::storage_port::{AgentBoundaryDeclaration, AgentStorageRead};

const VIOLATIONS_TOP_N: usize = 3;

pub fn aggregate<S: AgentStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	snapshot_uid: &str,
) -> Result<AggregatorOutput, AgentStorageError> {
	let declarations = storage.get_active_boundary_declarations(repo_uid)?;

	if declarations.is_empty() {
		return Ok(AggregatorOutput::empty());
	}

	// Deduplicate by (source_module, forbidden_target). The
	// first declaration for a given key wins — subsequent
	// duplicates are dropped. This prevents double-counting
	// violating edges when legacy authoring produced redundant
	// rows.
	let unique = dedupe_declarations(declarations);

	// For each unique rule, collect the violating edges and
	// aggregate into one per-rule entry. Total violation_count
	// is the sum of all edges across all unique rules.
	let mut per_rule: Vec<BoundaryViolationEvidence> = Vec::new();
	let mut total_edges: u64 = 0;

	for decl in unique {
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

	// Deterministic top-N ordering: sort descending by
	// edge_count, tiebreak by source_module then target_module
	// (lexicographic ascending). Then truncate to TOP_N.
	per_rule.sort_by(|a, b| {
		b.edge_count
			.cmp(&a.edge_count)
			.then_with(|| a.source_module.cmp(&b.source_module))
			.then_with(|| a.target_module.cmp(&b.target_module))
	});
	per_rule.truncate(VIOLATIONS_TOP_N);

	let evidence = BoundaryViolationsEvidence {
		violation_count: total_edges,
		top_violations: per_rule,
	};

	Ok(AggregatorOutput {
		signals: vec![Signal::boundary_violations(evidence)],
		limits: Vec::new(),
	})
}

/// Deduplicate a list of active boundary declarations by
/// `(source_module, forbidden_target)`. First-seen wins — the
/// `reason` carried by the surviving entry comes from the
/// first declaration the input iterator yields with that key.
///
/// Insertion order is preserved so downstream iteration is
/// deterministic. We deliberately do NOT use `HashMap::into_values`
/// (which has arbitrary order) nor sort the output here; the
/// caller sorts per-rule by `edge_count` after the violation
/// counts are known, which is where ordering actually matters.
fn dedupe_declarations(
	declarations: Vec<AgentBoundaryDeclaration>,
) -> Vec<AgentBoundaryDeclaration> {
	let mut seen: HashSet<(String, String)> = HashSet::new();
	let mut out: Vec<AgentBoundaryDeclaration> =
		Vec::with_capacity(declarations.len());
	for decl in declarations {
		let key = (decl.source_module.clone(), decl.forbidden_target.clone());
		if seen.insert(key) {
			out.push(decl);
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn dedupe_keeps_first_seen_reason_and_drops_duplicates() {
		let input = vec![
			AgentBoundaryDeclaration {
				source_module: "src/core".into(),
				forbidden_target: "src/adapters".into(),
				reason: Some("first-wins".into()),
			},
			AgentBoundaryDeclaration {
				source_module: "src/core".into(),
				forbidden_target: "src/adapters".into(),
				reason: Some("duplicate ignored".into()),
			},
			AgentBoundaryDeclaration {
				source_module: "src/core".into(),
				forbidden_target: "src/cli".into(),
				reason: None,
			},
		];
		let out = dedupe_declarations(input);
		assert_eq!(out.len(), 2);
		assert_eq!(out[0].source_module, "src/core");
		assert_eq!(out[0].forbidden_target, "src/adapters");
		assert_eq!(out[0].reason.as_deref(), Some("first-wins"));
		assert_eq!(out[1].source_module, "src/core");
		assert_eq!(out[1].forbidden_target, "src/cli");
	}

	#[test]
	fn dedupe_preserves_insertion_order_of_unique_entries() {
		let input = vec![
			AgentBoundaryDeclaration {
				source_module: "b".into(),
				forbidden_target: "b2".into(),
				reason: None,
			},
			AgentBoundaryDeclaration {
				source_module: "a".into(),
				forbidden_target: "a2".into(),
				reason: None,
			},
			AgentBoundaryDeclaration {
				source_module: "b".into(),
				forbidden_target: "b2".into(),
				reason: None,
			},
		];
		let out = dedupe_declarations(input);
		assert_eq!(out.len(), 2);
		// First-seen order: "b" before "a".
		assert_eq!(out[0].source_module, "b");
		assert_eq!(out[1].source_module, "a");
	}
}
